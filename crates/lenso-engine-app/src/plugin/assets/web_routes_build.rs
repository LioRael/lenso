//! Compile the selected Web Plugin's route files into one Endpoint provider.
//! This build script runs only while Cargo builds this Plugin. The resulting
//! native Host has a fixed route table and never scans source at runtime.

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use quote::quote;
use syn::{Attribute, Item, LitStr, Token, parse::Parser, punctuated::Punctuated};

const MAX_FILES: usize = 64;
const MAX_SOURCE_BYTES: u64 = 1024 * 1024;

fn main() {
    if let Err(error) = compile_routes() {
        panic!("Web route convention: {error}");
    }
}

fn compile_routes() -> Result<(), Box<dyn std::error::Error>> {
    let directory = Path::new("src/routes");
    println!("cargo:rerun-if-changed={}", directory.display());
    for part in [Path::new("src"), directory] {
        if !fs::symlink_metadata(part)?.file_type().is_dir() {
            return Err(format!(
                "route source must be a real directory, not a symlink or special file: {}",
                part.display()
            )
            .into());
        }
    }
    let mut files = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    files.sort_by_key(fs::DirEntry::file_name);
    let mut methods = Vec::new();
    let mut ids = BTreeMap::<String, PathBuf>::new();
    let mut paths = BTreeMap::<(String, String), PathBuf>::new();
    let mut total = 0_u64;
    for file in files {
        let path = file.path();
        let kind = file.file_type()?;
        if kind.is_symlink() || !kind.is_file() || path.extension().is_none_or(|ext| ext != "rs") {
            return Err(
                format!("route entry must be a regular .rs file: {}", path.display()).into(),
            );
        }
        if methods.len() >= MAX_FILES {
            return Err(format!("Web Plugin accepts at most {MAX_FILES} route files").into());
        }
        let size = file.metadata()?.len();
        total = total
            .checked_add(size)
            .ok_or("route source size overflow")?;
        if total > MAX_SOURCE_BYTES {
            return Err(format!("route sources exceed {MAX_SOURCE_BYTES} bytes").into());
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let source = fs::read_to_string(&path)?;
        let syntax =
            syn::parse_file(&source).map_err(|error| format!("{}: {error}", path.display()))?;
        if syntax.items.len() != 1 {
            return Err(format!(
                "{}: expected exactly one route handler function",
                path.display()
            )
            .into());
        }
        let Item::Fn(handler) = syntax.items.into_iter().next().expect("one item") else {
            return Err(format!("{}: expected a route handler function", path.display()).into());
        };
        let route = handler
            .attrs
            .iter()
            .filter_map(route_attribute)
            .collect::<Vec<_>>();
        if route.len() != 1 {
            return Err(format!(
                "{}: expected exactly one HTTP route attribute",
                path.display()
            )
            .into());
        }
        let (verb, attribute) = route[0];
        let values = Punctuated::<LitStr, Token![,]>::parse_terminated
            .parse2(attribute.meta.require_list()?.tokens.clone())?;
        if values.len() != 2 {
            return Err(format!("{}: route attribute needs an ID and path", path.display()).into());
        }
        let mut values = values.iter();
        let id = values.next().expect("two literals").value();
        let route_path = values.next().expect("two literals").value();
        if let Some(previous) = ids.insert(id.clone(), path.clone()) {
            return Err(format!(
                "duplicate Web route ID `{id}` in {} and {}",
                previous.display(),
                path.display()
            )
            .into());
        }
        if let Some(previous) = paths.insert((verb.to_owned(), route_path.clone()), path.clone()) {
            return Err(format!(
                "duplicate Web {verb} {route_path} in {} and {}",
                previous.display(),
                path.display()
            )
            .into());
        }
        methods.push(handler);
    }
    if methods.is_empty() {
        return Err("src/routes needs at least one .rs handler file".into());
    }
    let generated = quote! {
        #[endpoint]
        impl GreetingsHttp {
            #(#methods)*
        }
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
    fs::write(output.join("web_routes.rs"), generated.to_string())?;
    Ok(())
}

fn route_attribute(attribute: &Attribute) -> Option<(&'static str, &Attribute)> {
    let method = [
        "get", "post", "put", "patch", "delete", "head", "options", "query",
    ]
    .into_iter()
    .find(|method| attribute.path().is_ident(method))?;
    Some((
        match method {
            "get" => "GET",
            "post" => "POST",
            "put" => "PUT",
            "patch" => "PATCH",
            "delete" => "DELETE",
            "head" => "HEAD",
            "options" => "OPTIONS",
            "query" => "QUERY",
            _ => unreachable!(),
        },
        attribute,
    ))
}
