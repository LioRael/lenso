//! Optional Rust route authoring. Inputs and conventions are configurable;
//! lowering uses the existing Endpoint macro and HTTP execution stays in WebHost.
mod source;
use anyhow::{Context, ensure};
use lenso_engine::{ContextView, Plugin, Resource, Snapshot, Step};
use quote::quote;
use serde::{Deserialize, Serialize};
pub use source::{build, read_sources};
use std::collections::{BTreeMap, BTreeSet};
use syn::{Item, LitStr, Token, parse::Parser, punctuated::Punctuated};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebOptions {
    pub provider: String,
    pub roots: Vec<String>,
    /// Explicit Snapshot paths replace default root/extension discovery.
    pub entries: Vec<String>,
    pub exclude: Vec<String>,
    pub output: String,
    /// Empty providers are useful for applications with optional surfaces.
    pub allow_empty: bool,
    /// False uses official standalone lowering for a provider whose existing
    /// Plugin declaration already registers its grouped HTTP capabilities.
    pub register_plugin: bool,
}
impl Default for WebOptions {
    fn default() -> Self {
        Self {
            provider: "Http".into(),
            roots: vec!["src/routes".into()],
            entries: vec![],
            exclude: vec![],
            output: "web_routes.rs".into(),
            allow_empty: false,
            register_plugin: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub method: String,
    pub path: String,
    /// Diagnostic source location, supplied by any custom discovery/parser.
    pub source: String,
}

/// Public normalized input seam. Custom parsers can construct a RouteSet;
/// validation always runs and no runtime authority is inferred from these data.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RouteSet {
    pub routes: Vec<Route>,
}
impl RouteSet {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(self.routes.len() <= 256, "at most 256 routes");
        let mut ids = BTreeMap::new();
        let mut paths = BTreeMap::new();
        let mut routers = BTreeMap::<String, matchit::Router<String>>::new();
        for route in &self.routes {
            ensure!(!route.id.is_empty(), "{}: route ID required", route.source);
            ensure!(
                [
                    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "QUERY"
                ]
                .contains(&route.method.as_str()),
                "{}: unsupported method {}",
                route.source,
                route.method
            );
            let shape = path_shape(&route.path).with_context(|| route.source.clone())?;
            if let Some(previous) = ids.insert(&route.id, &route.source) {
                anyhow::bail!(
                    "duplicate route ID {}: {} and {}",
                    route.id,
                    previous,
                    route.source
                );
            }
            if let Some(previous) = paths.insert((route.method.clone(), shape), &route.source) {
                anyhow::bail!(
                    "conflicting {} {}: {} and {}",
                    route.method,
                    route.path,
                    previous,
                    route.source
                );
            }
            routers
                .entry(route.method.clone())
                .or_default()
                .insert(route.path.clone(), route.source.clone())
                .with_context(|| {
                    format!(
                        "{}: conflicting/invalid {} {}",
                        route.source, route.method, route.path
                    )
                })?;
        }
        Ok(())
    }
}

fn path_shape(path: &str) -> anyhow::Result<String> {
    ensure!(
        path.starts_with('/')
            && !path.contains(['?', '#', '\\'])
            && !path.chars().any(char::is_whitespace),
        "invalid route path {path}"
    );
    let mut names = BTreeSet::new();
    let mut shape = Vec::new();
    let parts: Vec<_> = path.split('/').skip(1).collect();
    for (index, part) in parts.iter().enumerate() {
        ensure!(!matches!(*part, "." | ".."), "invalid route path {path}");
        if part.starts_with('{') || part.ends_with('}') {
            ensure!(
                part.starts_with('{') && part.ends_with('}'),
                "invalid route parameter {path}"
            );
            let name = &part[1..part.len() - 1];
            let catch_all = name.starts_with('*');
            let name = name.strip_prefix('*').unwrap_or(name);
            ensure!(
                !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && names.insert(name),
                "invalid or duplicate path parameter {path}"
            );
            ensure!(
                !catch_all || index + 1 == parts.len(),
                "catch-all parameter must be final: {path}"
            );
            shape.push(if catch_all { "{*}" } else { "{}" });
        } else {
            ensure!(
                !part.contains(['{', '}', ':', '*']),
                "invalid route segment {path}"
            );
            shape.push(part);
        }
    }
    Ok(format!("/{}", shape.join("/")))
}

#[derive(Debug)]
pub struct WebAuthoring {
    options: WebOptions,
    identity: String,
}
impl WebAuthoring {
    pub fn new(options: WebOptions) -> anyhow::Result<Self> {
        syn::parse_str::<syn::TypePath>(&options.provider).context("invalid provider type")?;
        // Resource::file performs Engine's contained output-path validation.
        Resource::file(options.output.clone(), vec![])?;
        for path in options
            .roots
            .iter()
            .chain(&options.entries)
            .chain(&options.exclude)
        {
            Resource::file(path.clone(), vec![])?;
        }
        let identity = format!(
            "lenso.web.v1/{}/{}",
            env!("CARGO_PKG_VERSION"),
            serde_json::to_string(&options)?
        );
        Ok(Self { options, identity })
    }
}
impl Plugin for WebAuthoring {
    fn identity(&self) -> &str {
        &self.identity
    }
    fn cacheable(&self) -> bool {
        true
    }
    fn plan(&self, snapshot: &Snapshot) -> anyhow::Result<Vec<Step>> {
        let inputs = if self.options.entries.is_empty() {
            snapshot
                .files()
                .keys()
                .filter(|path| {
                    path.ends_with(".rs")
                        && self
                            .options
                            .roots
                            .iter()
                            .any(|root| path.starts_with(&format!("{root}/")))
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            self.options.entries.clone()
        };
        let inputs =
            inputs
                .into_iter()
                .filter(|path| {
                    !self.options.exclude.iter().any(|excluded| {
                        path == excluded || path.starts_with(&format!("{excluded}/"))
                    })
                })
                .collect::<Vec<_>>();
        ensure!(inputs.len() <= 256, "at most 256 route source files");
        ensure!(
            self.options.allow_empty || !inputs.is_empty(),
            "no selected route source; configure roots/entries or allow_empty"
        );
        Ok(vec![Step {
            id: "web/routes".into(),
            inputs,
            after: vec![],
            options: serde_json::to_value(&self.options)?,
        }])
    }
    fn process(&self, context: &ContextView<'_>) -> anyhow::Result<BTreeMap<String, Resource>> {
        let options: WebOptions = serde_json::from_value(context.step.options.clone())?;
        let mut routes = RouteSet::default();
        let mut methods = Vec::new();
        let mut size = 0;
        for (path, bytes) in &context.files {
            size += bytes.len();
            ensure!(size <= 1024 * 1024, "route source exceeds 1 MiB");
            let file =
                syn::parse_file(std::str::from_utf8(bytes)?).with_context(|| (*path).clone())?;
            for item in file.items {
                let Item::Fn(handler) = item else {
                    anyhow::bail!(
                        "{path}: route sources must contain handler functions; keep types/state in their owner module"
                    );
                };
                let attributes = handler
                    .attrs
                    .iter()
                    .filter_map(|attribute| {
                        let name = attribute.path().get_ident()?.to_string();
                        [
                            "get", "post", "put", "patch", "delete", "head", "options", "query",
                        ]
                        .contains(&name.as_str())
                        .then_some((name, attribute))
                    })
                    .collect::<Vec<_>>();
                ensure!(
                    attributes.len() == 1,
                    "{path}: handler {} needs one HTTP attribute",
                    handler.sig.ident
                );
                let (method, attribute) = &attributes[0];
                let values = Punctuated::<LitStr, Token![,]>::parse_terminated
                    .parse2(attribute.meta.require_list()?.tokens.clone())?;
                ensure!(
                    values.len() == 2,
                    "{path}: HTTP attribute needs route ID and path"
                );
                let mut values = values.iter();
                routes.routes.push(Route {
                    id: values.next().unwrap().value(),
                    method: method.to_uppercase(),
                    path: values.next().unwrap().value(),
                    source: format!("{path}:{}", handler.sig.ident),
                });
                methods.push(handler);
            }
        }
        ensure!(
            options.allow_empty || !routes.routes.is_empty(),
            "no selected route handlers"
        );
        routes.validate()?;
        let provider: syn::TypePath = syn::parse_str(&options.provider)?;
        let attribute = if options.register_plugin {
            quote!(#[lenso_capability_http_endpoint::endpoint])
        } else {
            quote!(#[lenso_capability_http_endpoint::endpoint(standalone)])
        };
        let source = if methods.is_empty() {
            "// @generated by lenso-engine-web\n".into()
        } else {
            format!(
                "// @generated by lenso-engine-web\n{}\n",
                quote! { #attribute impl #provider { #(#methods)* } }
            )
        };
        Ok(BTreeMap::from([
            (
                "routes".into(),
                Resource {
                    schema: "lenso.web-route-set.v1".into(),
                    value: serde_json::to_value(routes)?,
                },
            ),
            (
                "bindings".into(),
                Resource::file(options.output, source.into_bytes())?,
            ),
        ]))
    }
}

/// Embedding/build-script convenience. Supply a selected Snapshot to replace all
/// default filesystem behavior; this processor itself has no ambient file access.
pub fn compile(
    snapshot: Snapshot,
    options: WebOptions,
) -> anyhow::Result<lenso_engine::Generation> {
    let mut engine = lenso_engine::Engine::default();
    engine.register(WebAuthoring::new(options)?)?;
    let plan = engine.plan(snapshot)?;
    engine.execute(
        &plan,
        &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}
