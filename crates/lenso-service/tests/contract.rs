use lenso_service::{ModuleManifest, ServiceContract, ServiceHealth, ServiceProvider};

#[test]
fn service_contract_serializes_provider_and_modules() {
    let contract = ServiceContract::new(
        "support-suite-provider",
        vec![
            ModuleManifest::builder("support-ticket")
                .capabilities(vec!["support_ticket.tickets.read".to_owned()])
                .build(),
        ],
    )
    .version("0.2.0")
    .provider(ServiceProvider {
        name: "support-suite-provider".to_owned(),
        vendor: Some("Lenso".to_owned()),
        summary: Some("Support workflow provider".to_owned()),
        homepage: None,
    })
    .health(ServiceHealth {
        ready_url: Some("http://127.0.0.1:4110/lenso/service/v1/ready".to_owned()),
        status_url: Some("http://127.0.0.1:4110/lenso/service/v1/status".to_owned()),
        ..ServiceHealth::default()
    });

    let value = serde_json::to_value(contract).unwrap();

    assert_eq!(value["name"], "support-suite-provider");
    assert_eq!(value["version"], "0.2.0");
    assert_eq!(value["provider"]["vendor"], "Lenso");
    assert_eq!(
        value["health"]["readyUrl"],
        "http://127.0.0.1:4110/lenso/service/v1/ready"
    );
    assert_eq!(
        value["health"]["statusUrl"],
        "http://127.0.0.1:4110/lenso/service/v1/status"
    );
    assert_eq!(value["modules"][0]["name"], "support-ticket");

    let provider = value["provider"].as_object().unwrap();
    let health = value["health"].as_object().unwrap();
    assert!(!provider.contains_key("homepage"));
    assert!(!health.contains_key("manifestUrl"));
    assert!(!health.contains_key("livenessUrl"));
}
