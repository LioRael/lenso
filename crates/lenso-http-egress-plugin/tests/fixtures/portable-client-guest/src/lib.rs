use lenso_capability_http_client::{ClientGuestClient, SendRequest};
use serde_json::json;

wit_bindgen::generate!({
    path: "wit",
    world: "plugin",
});

lenso_guest_sdk::wasm_host!(struct WasmHost);

mod client {
    pub use lenso_capability_http_client::{CAPABILITY_ID, DESCRIPTOR_VERSION};
}

mod fixture {
    pub const CAPABILITY_ID: &str = "fixture.http-client@1";
    pub const DESCRIPTOR_VERSION: &str = "1.0.0";
    pub const RUN: &str = "run";
}

struct GuestComponent;

impl Guest for GuestComponent {
    fn describe() -> String {
        lenso_guest_sdk::guest_descriptor! {
            provides: [fixture {
                requests: [fixture::RUN],
                streams: [],
            }],
            requires: [("~lenso.http.client@1", client)],
        }
    }

    fn invoke(capability: String, operation: String, request_json: String) -> Result<String, String> {
        assert_eq!(capability, fixture::CAPABILITY_ID);
        assert_eq!(operation, fixture::RUN);
        let context = lenso_guest_sdk::GuestContext::load(WasmHost)
            .map_err(|error| format!("{error:?}"))?;
        let binding = context
            .require_named(
                "~lenso.http.client@1",
                client::CAPABILITY_ID,
                client::DESCRIPTOR_VERSION,
                &["send"],
                &[],
                &[],
            )
            .map_err(|error| format!("{error:?}"))?;
        let client = ClientGuestClient::from_context(&context)
            .map_err(|error| format!("{error:?}"))?;
        let request: SendRequest =
            serde_json::from_str(&request_json).map_err(|error| error.to_string())?;
        let result = client.send(&request);
        let response = match result {
            Ok(response) => json!({
                "provider": binding.binding().provider_instance(),
                "response": response,
            }),
            Err(lenso_guest_sdk::GuestError::Domain(error)) => json!({
                "provider": binding.binding().provider_instance(),
                "domain_error": error,
            }),
            Err(error) => return Err(format!("{error:?}")),
        };
        Ok(response.to_string())
    }

    fn stream_open(_: String, _: String, _: String) -> Result<u64, String> {
        Err("not supported".to_owned())
    }

    fn stream_send(_: u64, _: String) -> Result<(), String> {
        Err("not supported".to_owned())
    }

    fn stream_receive(_: u64) -> Result<String, String> {
        Err("not supported".to_owned())
    }

    fn stream_close_send(_: u64) -> Result<(), String> {
        Err("not supported".to_owned())
    }

    fn stream_cancel(_: u64) {}
}

export!(GuestComponent);
