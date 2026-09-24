use base64::{Engine as _, engine::general_purpose::STANDARD};
use lenso_portable_http_endpoint_fixture as handler;
use serde_json::{Value, json};

wit_bindgen::generate!({
    path: "wit",
    world: "plugin",
});

struct GuestComponent;

lenso_guest_sdk::guest_request_plugin! {
impl Guest for GuestComponent {
    provides: {
        capability_id: "lenso.http.endpoint@1",
        descriptor_version: "1.1.0",
        requests: ["describe", "handle"],
    }
    fn invoke(
        capability: String,
        operation: String,
        request_json: String,
    ) -> Result<String, String> {
        assert_eq!(capability, "lenso.http.endpoint@1");
        match operation.as_str() {
            "describe" => Ok(json!({
                "routes": handler::ROUTES.map(|(route_id, method, path)| json!({
                    "route_id": route_id,
                    "method": method,
                    "path": path,
                })),
            }).to_string()),
            "handle" => {
                let request: Value = serde_json::from_str(&request_json).unwrap();
                let body = STANDARD.decode(request["body"].as_str().unwrap()).unwrap();
                match handler::handle(
                    request["route_id"].as_str().unwrap(),
                    request["method"].as_str().unwrap(),
                    request["path"].as_str().unwrap(),
                    &body,
                    request["credential"]["scheme"].as_str(),
                    request["headers"]
                        .as_array()
                        .and_then(|headers| {
                            headers.iter().find(|header| {
                                header["name"]
                                    .as_str()
                                    .is_some_and(|name| name.eq_ignore_ascii_case("x-test"))
                            })
                        })
                        .and_then(|header| header["value"].as_str()),
                ) {
                    handler::Reply::Bytes(body) => Ok(json!({
                        "status": 200,
                        "headers": [],
                        "body": STANDARD.encode(body),
                    }).to_string()),
                    handler::Reply::DomainError => Err("\"rejected\"".to_owned()),
                    handler::Reply::RuntimeFailure => panic!("portable HTTP fixture failure"),
                }
            }
            _ => panic!("unexpected HTTP Endpoint operation"),
        }
    }
}
}

export!(GuestComponent);
