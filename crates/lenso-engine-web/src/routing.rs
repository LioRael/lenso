//! Directory conventions normalize into the same explicit Endpoint attributes.
use crate::WebOptions;
use anyhow::{Context, ensure};
use syn::{Attribute, ItemFn, LitStr, Meta, Token, parse::Parser, punctuated::Punctuated};

pub(super) fn normalize(
    source: &str,
    handler: &ItemFn,
    method: &str,
    attribute: &Attribute,
    options: &WebOptions,
) -> anyhow::Result<(String, String)> {
    let roots = options
        .filesystem_roots
        .iter()
        .filter(|root| source.starts_with(&format!("{root}/")))
        .collect::<Vec<_>>();
    ensure!(
        roots.len() <= 1,
        "{source}: overlapping filesystem roots are ambiguous"
    );
    let mut ids = handler
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("route_id"));
    let id = ids
        .next()
        .map(|attr| attr.parse_args::<LitStr>().map(|id| id.value()))
        .transpose()?;
    ensure!(
        ids.next().is_none(),
        "{source}: duplicate route_id attribute"
    );
    let values = match &attribute.meta {
        Meta::Path(_) => Punctuated::new(),
        _ => Punctuated::<LitStr, Token![,]>::parse_terminated
            .parse2(attribute.meta.require_list()?.tokens.clone())?,
    };
    if let Some(root) = roots.first() {
        ensure!(
            values.is_empty(),
            "{source}: filesystem routes require a bare HTTP method attribute; paths come from directories"
        );
        let path = filesystem_path(&source[root.len() + 1..]).with_context(|| source.to_owned())?;
        Ok((id.unwrap_or_else(|| format!("{method}:{path}")), path))
    } else {
        ensure!(
            (1..=2).contains(&values.len()),
            "{source}: HTTP attribute needs a path, or route ID and path"
        );
        ensure!(
            id.is_none() || values.len() == 1,
            "{source}: route ID declared twice"
        );
        let values = values
            .into_iter()
            .map(|value| value.value())
            .collect::<Vec<_>>();
        if values.len() == 2 {
            Ok((values[0].clone(), values[1].clone()))
        } else {
            Ok((
                id.unwrap_or_else(|| handler.sig.ident.to_string()),
                values[0].clone(),
            ))
        }
    }
}

fn filesystem_path(relative: &str) -> anyhow::Result<String> {
    let directory = relative
        .strip_suffix("route.rs")
        .context("filesystem route file must be named route.rs")?;
    ensure!(
        directory.is_empty() || directory.ends_with('/'),
        "invalid route filename"
    );
    let mut segments = Vec::new();
    let parts = directory
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        if part.starts_with('(') && part.ends_with(')') {
            let group = &part[1..part.len() - 1];
            ensure!(
                !group.is_empty()
                    && group
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "invalid route group {part}"
            );
        } else if part.starts_with('[') && part.ends_with(']') {
            let name = &part[1..part.len() - 1];
            let catch_all = name.starts_with("...");
            let name = name.strip_prefix("...").unwrap_or(name);
            ensure!(
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "invalid dynamic route {part}"
            );
            ensure!(
                !catch_all || index + 1 == parts.len(),
                "catch-all route must be final"
            );
            segments.push(if catch_all {
                format!("{{*{name}}}")
            } else {
                format!("{{{name}}}")
            });
        } else {
            ensure!(
                !matches!(*part, "." | "..")
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'),
                "invalid filesystem route segment {part}"
            );
            segments.push((*part).to_owned());
        }
    }
    Ok(format!("/{}", segments.join("/")))
}
