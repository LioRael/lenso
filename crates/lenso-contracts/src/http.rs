//! Pure-data HTTP route declarations for module manifests.
//!
//! These declarations are metadata only. Linked modules still contribute real
//! Axum/OpenAPI routes through `lenso-bootstrap`; remote route proxying requires a
//! separate host protocol before these entries can be mounted.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::module_source::ModuleSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
#[non_exhaustive]
pub enum ModuleHttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ModuleHttpRoute {
    pub method: ModuleHttpMethod,
    /// Module-local path, e.g. `/contacts` or `/contacts/{id}`.
    pub path: String,
    /// Optional capability required before a future host proxy exposes this
    /// route. No enforcement exists until the proxy protocol is implemented.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// Compact label used when this route appears as a runtime story node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Optional story title for direct requests to this route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<crate::ServiceOperationMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleRouteLintSeverity {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ModuleRouteLint {
    pub severity: ModuleRouteLintSeverity,
    pub subject: String,
    pub message: String,
    pub suggestion: String,
}

pub fn lint_module_http_routes(
    source: ModuleSource,
    routes: &[ModuleHttpRoute],
) -> Vec<ModuleRouteLint> {
    if routes.is_empty() {
        return vec![ModuleRouteLint {
            severity: if source == ModuleSource::Remote {
                ModuleRouteLintSeverity::Warning
            } else {
                ModuleRouteLintSeverity::Ok
            },
            subject: "routes".to_owned(),
            message: "No HTTP interfaces are declared in this manifest.".to_owned(),
            suggestion: if source == ModuleSource::Remote {
                "Add ModuleHttpRoute declarations for remote HTTP interfaces that should be visible to the host."
            } else {
                "No action needed unless this linked module owns public HTTP routes."
            }
            .to_owned(),
        }];
    }

    let mut lints = Vec::new();
    let mut route_counts = HashMap::<String, usize>::new();
    for route in routes {
        *route_counts.entry(route_identity(route)).or_default() += 1;
    }

    for (identity, count) in route_counts.iter().filter(|(_, count)| **count > 1) {
        lints.push(ModuleRouteLint {
            severity: ModuleRouteLintSeverity::Error,
            subject: identity.clone(),
            message: format!("{count} routes declare the same method and path."),
            suggestion: "Keep one route declaration per method and path.".to_owned(),
        });
    }

    for (index, route) in routes.iter().enumerate() {
        let identity = route_identity(route);
        if !present(route.display_name.as_deref()) {
            lints.push(ModuleRouteLint {
                severity: ModuleRouteLintSeverity::Warning,
                subject: identity.clone(),
                message: "Missing display_name for compact runtime story nodes.".to_owned(),
                suggestion:
                    "Add display_name to ModuleHttpRoute for compact story timeline labels."
                        .to_owned(),
            });
        }
        if !present(route.story_title.as_deref()) {
            lints.push(ModuleRouteLint {
                severity: ModuleRouteLintSeverity::Warning,
                subject: identity.clone(),
                message: "Missing story_title for direct HTTP entry stories.".to_owned(),
                suggestion: "Add story_title when this route can be a direct business entry."
                    .to_owned(),
            });
        }
        if source == ModuleSource::Remote && !present(route.capability.as_deref()) {
            lints.push(ModuleRouteLint {
                severity: ModuleRouteLintSeverity::Warning,
                subject: identity.clone(),
                message: "Missing capability declaration for host proxy authorization.".to_owned(),
                suggestion:
                    "Remote routes should declare the capability used by host proxy authorization."
                        .to_owned(),
            });
        }

        if index == routes.len() - 1 && lints.is_empty() {
            lints.push(ModuleRouteLint {
                severity: ModuleRouteLintSeverity::Ok,
                subject: "routes".to_owned(),
                message: if source == ModuleSource::Remote {
                    "Declared routes include display, story, and capability metadata."
                } else {
                    "Declared routes include display and story metadata."
                }
                .to_owned(),
                suggestion: "No action needed.".to_owned(),
            });
        }
    }

    lints
}

fn route_identity(route: &ModuleHttpRoute) -> String {
    format!("{} {}", method_label(route.method), route.path)
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn method_label(method: ModuleHttpMethod) -> &'static str {
    match method {
        ModuleHttpMethod::Get => "GET",
        ModuleHttpMethod::Post => "POST",
        ModuleHttpMethod::Put => "PUT",
        ModuleHttpMethod::Patch => "PATCH",
        ModuleHttpMethod::Delete => "DELETE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(method: ModuleHttpMethod, path: &str) -> ModuleHttpRoute {
        ModuleHttpRoute {
            method,
            path: path.to_owned(),
            capability: None,
            display_name: None,
            story_title: None,
            operation: None,
        }
    }

    #[test]
    fn linked_routes_do_not_require_capability() {
        let mut route = route(ModuleHttpMethod::Post, "/v1/identity/users");
        route.display_name = Some("Create User Request".to_owned());
        route.story_title = Some("User Registration".to_owned());

        assert_eq!(
            lint_module_http_routes(ModuleSource::Linked, &[route]),
            vec![ModuleRouteLint {
                severity: ModuleRouteLintSeverity::Ok,
                subject: "routes".to_owned(),
                message: "Declared routes include display and story metadata.".to_owned(),
                suggestion: "No action needed.".to_owned(),
            }]
        );
    }

    #[test]
    fn remote_routes_require_capability() {
        let mut route = route(ModuleHttpMethod::Get, "/contacts/{id}");
        route.display_name = Some("Fetch Contact".to_owned());
        route.story_title = Some("Fetch Contact".to_owned());

        assert_eq!(
            lint_module_http_routes(ModuleSource::Remote, &[route]),
            vec![ModuleRouteLint {
                severity: ModuleRouteLintSeverity::Warning,
                subject: "GET /contacts/{id}".to_owned(),
                message: "Missing capability declaration for host proxy authorization.".to_owned(),
                suggestion:
                    "Remote routes should declare the capability used by host proxy authorization."
                        .to_owned(),
            }]
        );
    }

    #[test]
    fn duplicate_routes_are_errors() {
        assert_eq!(
            lint_module_http_routes(
                ModuleSource::Remote,
                &[
                    route(ModuleHttpMethod::Get, "/contacts/{id}"),
                    route(ModuleHttpMethod::Get, "/contacts/{id}"),
                ],
            )[0],
            ModuleRouteLint {
                severity: ModuleRouteLintSeverity::Error,
                subject: "GET /contacts/{id}".to_owned(),
                message: "2 routes declare the same method and path.".to_owned(),
                suggestion: "Keep one route declaration per method and path.".to_owned(),
            }
        );
    }

    #[test]
    fn remote_empty_routes_are_warnings() {
        assert_eq!(
            lint_module_http_routes(ModuleSource::Remote, &[]),
            vec![ModuleRouteLint {
                severity: ModuleRouteLintSeverity::Warning,
                subject: "routes".to_owned(),
                message: "No HTTP interfaces are declared in this manifest.".to_owned(),
                suggestion:
                    "Add ModuleHttpRoute declarations for remote HTTP interfaces that should be visible to the host."
                        .to_owned(),
            }]
        );
    }
}
