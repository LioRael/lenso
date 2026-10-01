//! Keep local contract macros hygienic when several projections share a crate.
use crate::{CodegenError, GeneratedProjection, ProjectionLanguage, generate_projection};
use proc_macro2::{TokenStream, TokenTree};
use std::{fmt::Write as _, path::Path};

fn invalid(detail: impl Into<String>) -> CodegenError {
    CodegenError::InvalidDescriptor {
        detail: detail.into(),
    }
}

/// Generate a Rust projection loaded as a normal module through `#[path]`.
/// `module` is the exact public path from the consuming crate's root.
pub fn generate_module_projection(
    path: &Path,
    language: ProjectionLanguage,
    module: &str,
) -> Result<GeneratedProjection, CodegenError> {
    if !matches!(
        language,
        ProjectionLanguage::Rust | ProjectionLanguage::RustRuntime
    ) {
        return Err(invalid("module projections require rust or rust-runtime"));
    }
    let parsed: syn::Path =
        syn::parse_str(module).map_err(|_| invalid("invalid Rust contract module path"))?;
    if parsed.leading_colon.is_some()
        || parsed.segments.iter().any(|s| {
            !matches!(s.arguments, syn::PathArguments::None)
                || matches!(
                    s.ident.to_string().as_str(),
                    "crate" | "self" | "super" | "Self"
                )
        })
    {
        return Err(invalid(
            "contract module must be a relative named path from the crate root",
        ));
    }
    let module = parsed
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    let mut prefix = String::new();
    for segment in &parsed.segments {
        let name = segment.ident.to_string();
        let name = name.strip_prefix("r#").unwrap_or(&name);
        write!(prefix, "_{}_{}", name.len(), name).expect("String writes do not fail");
    }
    let mut projection = generate_projection(path, language)?;
    let file = syn::parse_file(&projection.source)
        .map_err(|e| invalid(format!("invalid generated Rust projection: {e}")))?;
    let mut replacements = Vec::new();
    let mut aliases = String::new();
    for item in file.items {
        if let syn::Item::Macro(item) = item {
            if !item.mac.path.is_ident("macro_rules") {
                continue;
            }
            let Some(name) = item.ident else {
                continue;
            };
            let exported = format!("__lenso_contract{prefix}_{name}");
            replacements.push((name.span().byte_range(), exported.clone()));
            write!(
                aliases,
                "#[doc(hidden)]\npub use crate::{exported} as {name};\n"
            )
            .expect("String writes do not fail");
            namespace(item.mac.tokens, &module, &mut replacements);
        }
    }
    // Token spans let us change paths and names without changing string literals
    // or the generated contract's types, schemas, descriptors and wire metadata.
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in replacements {
        projection.source.replace_range(range, &replacement);
    }
    projection.source.push_str(&aliases);
    Ok(projection)
}

fn namespace(
    tokens: TokenStream,
    module: &str,
    replacements: &mut Vec<(std::ops::Range<usize>, String)>,
) {
    let tokens = tokens.into_iter().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if let TokenTree::Group(group) = token {
            namespace(group.stream(), module, replacements);
        }
        if let TokenTree::Ident(name) = token
            && name == "crate"
            && index > 0
            && matches!(&tokens[index - 1], TokenTree::Punct(p) if p.as_char() == '$')
        {
            replacements.push((name.span().byte_range(), format!("crate::{module}")));
        }
    }
}

/// Write an exact, reproducible in-package Rust projection.
pub fn write_module_projection(
    descriptor: &Path,
    language: ProjectionLanguage,
    module: &str,
    output: &Path,
) -> Result<(), CodegenError> {
    crate::write_artifact(
        output,
        &generate_module_projection(descriptor, language, module)?.source,
    )
}

/// Check an in-package projection without editing generated files.
pub fn check_module_projection(
    descriptor: &Path,
    language: ProjectionLanguage,
    module: &str,
    output: &Path,
) -> Result<(), CodegenError> {
    crate::check_artifact(
        output,
        &generate_module_projection(descriptor, language, module)?.source,
    )
}
