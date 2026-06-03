use crate::config::RemoteModuleConfig;
use platform_module::{Module, ModuleHttpMethod, ModuleHttpRoute, ModuleSource};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHttpProxyRegistry {
    modules: BTreeMap<String, RemoteHttpProxyModule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHttpProxyModule {
    pub module_name: String,
    pub base_url: String,
    pub routes: Vec<RemoteHttpProxyRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHttpProxyRoute {
    pub method: ModuleHttpMethod,
    pub declared_path: String,
    pub capability: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHttpProxyMatch {
    pub module_name: String,
    pub base_url: String,
    pub method: ModuleHttpMethod,
    pub declared_path: String,
    pub remote_path: String,
    pub capability: Option<String>,
    pub path_params: BTreeMap<String, String>,
}

impl RemoteHttpProxyRegistry {
    #[must_use]
    pub fn from_modules(modules: &[Module], configs: &[RemoteModuleConfig]) -> Self {
        let config_by_name: BTreeMap<_, _> = configs
            .iter()
            .map(|config| (config.name.as_str(), config))
            .collect();
        let modules = modules
            .iter()
            .filter(|module| module.source == ModuleSource::Remote)
            .filter_map(|module| {
                let config = config_by_name.get(module.manifest.name.as_str())?;
                let routes = module
                    .manifest
                    .http_routes
                    .iter()
                    .filter_map(RemoteHttpProxyRoute::from_manifest_route)
                    .collect::<Vec<_>>();
                if routes.is_empty() {
                    return None;
                }
                Some((
                    module.manifest.name.clone(),
                    RemoteHttpProxyModule {
                        module_name: module.manifest.name.clone(),
                        base_url: config.base_url.clone(),
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
    pub fn modules(&self) -> impl Iterator<Item = &RemoteHttpProxyModule> {
        self.modules.values()
    }

    #[must_use]
    pub fn match_route(
        &self,
        module_name: &str,
        method: ModuleHttpMethod,
        request_path: &str,
    ) -> Option<RemoteHttpProxyMatch> {
        let module = self.modules.get(module_name)?;
        let normalized_path = normalize_request_path(request_path)?;
        module.routes.iter().find_map(|route| {
            if route.method != method {
                return None;
            }
            let path_params = match_declared_path(&route.declared_path, &normalized_path)?;
            Some(RemoteHttpProxyMatch {
                module_name: module.module_name.clone(),
                base_url: module.base_url.clone(),
                method: route.method,
                declared_path: route.declared_path.clone(),
                remote_path: normalized_path.clone(),
                capability: route.capability.clone(),
                path_params,
            })
        })
    }
}

impl RemoteHttpProxyRoute {
    fn from_manifest_route(route: &ModuleHttpRoute) -> Option<Self> {
        validate_declared_path_pattern(&route.path)?;
        Some(Self {
            method: route.method,
            declared_path: route.path.clone(),
            capability: route.capability.clone(),
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
            capability: Some("remote_crm.contacts.read".to_owned()),
        }
    }

    fn remote_module(name: &str, routes: Vec<ModuleHttpRoute>) -> Module {
        Module::remote(
            ModuleManifest::builder(name).http_routes(routes).build(),
            std::sync::Arc::new(crate::RemoteBinding),
        )
    }

    #[test]
    fn registry_includes_remote_modules_with_valid_routes() {
        let modules = vec![
            remote_module(
                "remote-crm",
                vec![
                    route(ModuleHttpMethod::Get, "/contacts"),
                    route(ModuleHttpMethod::Get, "/contacts/{id}"),
                ],
            ),
            Module::linked(
                ModuleManifest::builder("identity")
                    .http_routes(vec![route(ModuleHttpMethod::Get, "/users")])
                    .build(),
                LinkedBinding::builder().build(),
            ),
        ];
        let registry = RemoteHttpProxyRegistry::from_modules(
            &modules,
            &[RemoteModuleConfig::new(
                "remote-crm",
                "http://127.0.0.1:4100/lenso/module/v1",
            )],
        );

        assert_eq!(registry.modules().count(), 1);
        let module = registry.modules().next().expect("remote module");
        assert_eq!(module.module_name, "remote-crm");
        assert_eq!(module.routes.len(), 2);
    }

    #[test]
    fn matcher_extracts_single_segment_params() {
        let registry = RemoteHttpProxyRegistry::from_modules(
            &[remote_module(
                "remote-crm",
                vec![route(ModuleHttpMethod::Get, "/contacts/{id}")],
            )],
            &[RemoteModuleConfig::new(
                "remote-crm",
                "http://127.0.0.1:4100/lenso/module/v1",
            )],
        );

        let matched = registry
            .match_route("remote-crm", ModuleHttpMethod::Get, "/contacts/contact_1")
            .expect("route should match");

        assert_eq!(matched.declared_path, "/contacts/{id}");
        assert_eq!(matched.remote_path, "/contacts/contact_1");
        assert_eq!(
            matched.path_params.get("id").map(String::as_str),
            Some("contact_1")
        );
        assert_eq!(
            matched.capability.as_deref(),
            Some("remote_crm.contacts.read")
        );
    }

    #[test]
    fn matcher_rejects_wrong_method_module_and_shape() {
        let registry = RemoteHttpProxyRegistry::from_modules(
            &[remote_module(
                "remote-crm",
                vec![route(ModuleHttpMethod::Get, "/contacts/{id}")],
            )],
            &[RemoteModuleConfig::new(
                "remote-crm",
                "http://127.0.0.1:4100/lenso/module/v1",
            )],
        );

        assert!(
            registry
                .match_route("remote-crm", ModuleHttpMethod::Post, "/contacts/contact_1")
                .is_none()
        );
        assert!(
            registry
                .match_route("other", ModuleHttpMethod::Get, "/contacts/contact_1")
                .is_none()
        );
        assert!(
            registry
                .match_route("remote-crm", ModuleHttpMethod::Get, "/contacts")
                .is_none()
        );
    }

    #[test]
    fn registry_drops_invalid_declared_patterns() {
        let registry = RemoteHttpProxyRegistry::from_modules(
            &[remote_module(
                "remote-crm",
                vec![
                    route(ModuleHttpMethod::Get, "/contacts/{id}/{id}"),
                    route(ModuleHttpMethod::Get, "/contacts/{id"),
                    route(ModuleHttpMethod::Get, "/contacts/*tail"),
                ],
            )],
            &[RemoteModuleConfig::new(
                "remote-crm",
                "http://127.0.0.1:4100/lenso/module/v1",
            )],
        );

        assert!(registry.is_empty());
    }

    #[test]
    fn matcher_rejects_unsafe_request_paths() {
        let registry = RemoteHttpProxyRegistry::from_modules(
            &[remote_module(
                "remote-crm",
                vec![route(ModuleHttpMethod::Get, "/contacts/{id}")],
            )],
            &[RemoteModuleConfig::new(
                "remote-crm",
                "http://127.0.0.1:4100/lenso/module/v1",
            )],
        );

        for path in [
            "contacts/contact_1",
            "//contacts/contact_1",
            "/contacts/../secret",
            "/contacts/contact_1?x=1",
            "/contacts/contact_1#frag",
        ] {
            assert!(
                registry
                    .match_route("remote-crm", ModuleHttpMethod::Get, path)
                    .is_none(),
                "{path} should not match"
            );
        }
    }
}
