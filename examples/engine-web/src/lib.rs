use lenso_capability_http_endpoint::prelude::*;

#[lenso::plugin(id = "example.engine-web", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {}

#[derive(Debug, serde::Deserialize)]
struct ItemPath {
    id: String,
}

include!(concat!(env!("OUT_DIR"), "/web_routes.rs"));

#[cfg(test)]
mod tests {
    #[test]
    fn generated_routes_compile_against_the_official_endpoint_macro() {
        use lenso_capability_http_endpoint::HttpEndpoint;
        assert_eq!(super::Plugin::ROUTES.len(), 2);
        assert_eq!(super::Plugin::ROUTES[1].path(), "/items/{id}");
    }
}
