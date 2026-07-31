use crate::config::{ProviderConfig, ProviderTransport};
use platform_module::{Module, ModuleHttpMethod, ModuleHttpRoute, ModuleSource};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHttpProxyRegistry {
    modules: BTreeMap<String, ProviderHttpProxyModule>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderHttpProxyModule {
    pub(crate) config: ProviderConfig,
    pub module_name: String,
    pub base_url: String,
    pub transport: ProviderTransport,
    pub timeout_ms: u64,
    pub(crate) auth_token: Option<String>,
    pub routes: Vec<ProviderHttpProxyRoute>,
}

impl std::fmt::Debug for ProviderHttpProxyModule {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderHttpProxyModule")
            .field("module_name", &self.module_name)
            .field("base_url", &self.base_url)
            .field("transport", &self.transport)
            .field("timeout_ms", &self.timeout_ms)
            .field("auth_configured", &self.auth_token.is_some())
            .field("routes", &self.routes)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderHttpProxyRoute {
    pub method: ModuleHttpMethod,
    pub declared_path: String,
    pub capability: Option<String>,
    pub display_name: Option<String>,
    pub story_title: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderHttpProxyMatch {
    pub(crate) config: ProviderConfig,
    pub module_name: String,
    pub base_url: String,
    pub(crate) transport: ProviderTransport,
    pub(crate) timeout_ms: u64,
    pub(crate) auth_token: Option<String>,
    pub method: ModuleHttpMethod,
    pub declared_path: String,
    pub provider_path: String,
    pub capability: Option<String>,
    pub display_name: Option<String>,
    pub story_title: Option<String>,
    pub path_params: BTreeMap<String, String>,
}

impl std::fmt::Debug for ProviderHttpProxyMatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderHttpProxyMatch")
            .field("module_name", &self.module_name)
            .field("base_url", &self.base_url)
            .field("transport", &self.transport)
            .field("timeout_ms", &self.timeout_ms)
            .field("auth_configured", &self.auth_token.is_some())
            .field("method", &self.method)
            .field("declared_path", &self.declared_path)
            .field("provider_path", &self.provider_path)
            .field("capability", &self.capability)
            .field("display_name", &self.display_name)
            .field("story_title", &self.story_title)
            .field("path_params", &self.path_params)
            .finish()
    }
}

impl ProviderHttpProxyRegistry {
    #[must_use]
    pub fn from_modules(modules: &[Module], configs: &[ProviderConfig]) -> Self {
        let modules = modules
            .iter()
            .filter(|module| module.source == ModuleSource::Service)
            .filter_map(|module| {
                let config = configs
                    .iter()
                    .find(|config| config.matches_module_id(&module.manifest.module_id))?;
                let routes = module
                    .manifest
                    .http_routes
                    .iter()
                    .filter_map(ProviderHttpProxyRoute::from_manifest_route)
                    .collect::<Vec<_>>();
                if routes.is_empty() {
                    return None;
                }
                Some((
                    config.name.clone(),
                    ProviderHttpProxyModule {
                        config: config.clone(),
                        module_name: config.name.clone(),
                        base_url: config.base_url.clone(),
                        transport: config.transport,
                        timeout_ms: config.timeout_ms,
                        auth_token: config.auth_token.clone(),
                        routes,
                    },
                ))
            })
            .collect();
        Self { modules }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    #[must_use]
    pub fn modules(&self) -> impl Iterator<Item = &ProviderHttpProxyModule> {
        self.modules.values()
    }

    #[must_use]
    pub fn match_route(
        &self,
        module_name: &str,
        method: ModuleHttpMethod,
        request_path: &str,
    ) -> Option<ProviderHttpProxyMatch> {
        let module = self.modules.get(module_name)?;
        let normalized_path = normalize_request_path(request_path)?;
        module.routes.iter().find_map(|route| {
            if route.method != method {
                return None;
            }
            let path_params = match_declared_path(&route.declared_path, &normalized_path)?;
            Some(ProviderHttpProxyMatch {
                config: module.config.clone(),
                module_name: module.module_name.clone(),
                base_url: module.base_url.clone(),
                transport: module.transport,
                timeout_ms: module.timeout_ms,
                auth_token: module.auth_token.clone(),
                method: route.method,
                declared_path: route.declared_path.clone(),
                provider_path: normalized_path.clone(),
                capability: route.capability.clone(),
                display_name: route.display_name.clone(),
                story_title: route.story_title.clone(),
                path_params,
            })
        })
    }
}

impl ProviderHttpProxyRoute {
    fn from_manifest_route(route: &ModuleHttpRoute) -> Option<Self> {
        validate_declared_path_pattern(&route.path)?;
        Some(Self {
            method: route.method,
            declared_path: route.path.clone(),
            capability: route.capability.clone(),
            display_name: route.display_name.clone(),
            story_title: route.story_title.clone(),
        })
    }
}

fn validate_declared_path_pattern(path: &str) -> Option<()> {
    let segments = normalized_segments(path)?;
    let mut params = HashSet::new();
    for segment in segments {
        if is_parameter_segment(segment) {
            let name = &segment[1..segment.len() - 1];
            if name.is_empty() || !is_identifier(name) || !params.insert(name.to_owned()) {
                return None;
            }
        } else if segment.contains('{') || segment.contains('}') || segment.contains('*') {
            return None;
        }
    }
    Some(())
}

fn match_declared_path(
    declared_path: &str,
    request_path: &str,
) -> Option<BTreeMap<String, String>> {
    let declared_segments = normalized_segments(declared_path)?;
    let request_segments = normalized_segments(request_path)?;
    if declared_segments.len() != request_segments.len() {
        return None;
    }

    let mut params = BTreeMap::new();
    for (declared, requested) in declared_segments.iter().zip(request_segments) {
        if is_parameter_segment(declared) {
            let name = &declared[1..declared.len() - 1];
            params.insert(name.to_owned(), requested.to_owned());
        } else if *declared != requested {
            return None;
        }
    }
    Some(params)
}

fn normalize_request_path(path: &str) -> Option<String> {
    let segments = normalized_segments(path)?;
    Some(format!("/{}", segments.join("/")))
}

fn normalized_segments(path: &str) -> Option<Vec<&str>> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains('\\')
        || path.contains("://")
        || path.contains('?')
        || path.contains('#')
    {
        return None;
    }
    let segments = path.split('/').skip(1).collect::<Vec<_>>();
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return None;
    }
    Some(segments)
}

fn is_parameter_segment(segment: &str) -> bool {
    segment.starts_with('{') && segment.ends_with('}')
}

fn is_identifier(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch == '_' || ch == '-' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{LinkedBinding, ModuleManifest};

    fn route(method: ModuleHttpMethod, path: &str) -> ModuleHttpRoute {
        ModuleHttpRoute {
            method,
            path: path.to_owned(),
            capability: Some("provider_crm.contacts.read".to_owned()),
            display_name: None,
            story_title: None,
            operation: None,
        }
    }

    fn provider(name: &str, routes: Vec<ModuleHttpRoute>) -> Module {
        Module::service(
            ModuleManifest::builder(name).http_routes(routes).build(),
            std::sync::Arc::new(crate::ProviderBinding::default()),
        )
    }

    #[test]
    fn registry_includes_providers_with_valid_routes() {
        let modules = vec![
            provider(
                "provider-crm",
                vec![
                    route(ModuleHttpMethod::Get, "/contacts"),
                    route(ModuleHttpMethod::Get, "/contacts/{id}"),
                ],
            ),
            Module::linked(
                ModuleManifest::builder("lenso/identity")
                    .http_routes(vec![route(ModuleHttpMethod::Get, "/users")])
                    .build(),
                LinkedBinding::builder().build(),
            ),
        ];
        let registry = ProviderHttpProxyRegistry::from_modules(
            &modules,
            &[ProviderConfig::new(
                "provider-crm",
                "http://127.0.0.1:4100/lenso/provider/v1",
            )],
        );

        assert_eq!(registry.modules().count(), 1);
        let module = registry.modules().next().expect("Provider Service");
        assert_eq!(module.module_name, "provider-crm");
        assert_eq!(module.routes.len(), 2);
    }

    #[test]
    fn registry_preserves_configured_provider_auth_token() {
        let registry = ProviderHttpProxyRegistry::from_modules(
            &[provider(
                "provider-crm",
                vec![route(ModuleHttpMethod::Get, "/contacts/{id}")],
            )],
            &[
                ProviderConfig::new("provider-crm", "http://127.0.0.1:4100/lenso/provider/v1")
                    .with_timeout_ms(250)
                    .with_auth_token("provider-secret"),
            ],
        );

        let matched = registry
            .match_route("provider-crm", ModuleHttpMethod::Get, "/contacts/contact_1")
            .expect("route should match");
        assert_eq!(matched.timeout_ms, 250);
        assert_eq!(matched.auth_token.as_deref(), Some("provider-secret"));
    }

    #[test]
    fn registry_includes_grpc_providers() {
        let registry = ProviderHttpProxyRegistry::from_modules(
            &[provider(
                "provider-crm",
                vec![route(ModuleHttpMethod::Get, "/contacts")],
            )],
            &[ProviderConfig::new(
                "provider-crm",
                "grpc://127.0.0.1:50051",
            )],
        );

        let matched = registry
            .match_route("provider-crm", ModuleHttpMethod::Get, "/contacts")
            .expect("route should match");
        assert_eq!(matched.transport, ProviderTransport::Grpc);
        assert_eq!(matched.base_url, "http://127.0.0.1:50051");
    }

    #[test]
    fn matcher_extracts_single_segment_params() {
        let registry = ProviderHttpProxyRegistry::from_modules(
            &[provider(
                "provider-crm",
                vec![route(ModuleHttpMethod::Get, "/contacts/{id}")],
            )],
            &[ProviderConfig::new(
                "provider-crm",
                "http://127.0.0.1:4100/lenso/provider/v1",
            )],
        );

        let matched = registry
            .match_route("provider-crm", ModuleHttpMethod::Get, "/contacts/contact_1")
            .expect("route should match");

        assert_eq!(matched.declared_path, "/contacts/{id}");
        assert_eq!(matched.provider_path, "/contacts/contact_1");
        assert_eq!(
            matched.path_params.get("id").map(String::as_str),
            Some("contact_1")
        );
        assert_eq!(
            matched.capability.as_deref(),
            Some("provider_crm.contacts.read")
        );
        assert_eq!(matched.display_name, None);
        assert_eq!(matched.story_title, None);
    }

    #[test]
    fn matcher_preserves_route_display_metadata() {
        let mut route = route(ModuleHttpMethod::Get, "/contacts/{id}");
        route.display_name = Some("Fetch Contact".to_owned());
        route.story_title = Some("Fetch Contact".to_owned());
        let registry = ProviderHttpProxyRegistry::from_modules(
            &[provider("provider-crm", vec![route])],
            &[ProviderConfig::new(
                "provider-crm",
                "http://127.0.0.1:4100/lenso/provider/v1",
            )],
        );

        let matched = registry
            .match_route("provider-crm", ModuleHttpMethod::Get, "/contacts/contact_1")
            .expect("route should match");

        assert_eq!(matched.display_name.as_deref(), Some("Fetch Contact"));
        assert_eq!(matched.story_title.as_deref(), Some("Fetch Contact"));
    }

    #[test]
    fn matcher_rejects_wrong_method_module_and_shape() {
        let registry = ProviderHttpProxyRegistry::from_modules(
            &[provider(
                "provider-crm",
                vec![route(ModuleHttpMethod::Get, "/contacts/{id}")],
            )],
            &[ProviderConfig::new(
                "provider-crm",
                "http://127.0.0.1:4100/lenso/provider/v1",
            )],
        );

        assert!(
            registry
                .match_route(
                    "provider-crm",
                    ModuleHttpMethod::Post,
                    "/contacts/contact_1"
                )
                .is_none()
        );
        assert!(
            registry
                .match_route("other", ModuleHttpMethod::Get, "/contacts/contact_1")
                .is_none()
        );
        assert!(
            registry
                .match_route("provider-crm", ModuleHttpMethod::Get, "/contacts")
                .is_none()
        );
    }

    #[test]
    fn registry_drops_invalid_declared_patterns() {
        let registry = ProviderHttpProxyRegistry::from_modules(
            &[provider(
                "provider-crm",
                vec![
                    route(ModuleHttpMethod::Get, "/contacts/{id}/{id}"),
                    route(ModuleHttpMethod::Get, "/contacts/{id"),
                    route(ModuleHttpMethod::Get, "/contacts/*tail"),
                ],
            )],
            &[ProviderConfig::new(
                "provider-crm",
                "http://127.0.0.1:4100/lenso/provider/v1",
            )],
        );

        assert!(registry.is_empty());
    }

    #[test]
    fn matcher_rejects_unsafe_request_paths() {
        let registry = ProviderHttpProxyRegistry::from_modules(
            &[provider(
                "provider-crm",
                vec![route(ModuleHttpMethod::Get, "/contacts/{id}")],
            )],
            &[ProviderConfig::new(
                "provider-crm",
                "http://127.0.0.1:4100/lenso/provider/v1",
            )],
        );

        for path in [
            "contacts/contact_1",
            "//contacts/contact_1",
            "/contacts/../secret",
            "/contacts/..\\admin",
            "/contacts\\..\\admin",
            "/contacts/\\evil.example",
            "/contacts/contact_1?x=1",
            "/contacts/contact_1#frag",
        ] {
            assert!(
                registry
                    .match_route("provider-crm", ModuleHttpMethod::Get, path)
                    .is_none(),
                "{path} should not match"
            );
        }
    }
}
