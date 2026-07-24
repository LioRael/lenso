# Lenso GA Support Manifest

- Protocol: `lenso.ga-support-manifest.v1`
- Manifest ID: `ga-support:92812a6fdfb333b2`
- Manifest digest: `sha256:92812a6fdfb333b229e6867d0cac8b0b757bd2f844fb9d0f12a4c49fe78740c4`
- Status: `GeneralAvailability`
- Documentation: `m6-ga` (`sha256:09aeded7e3eb0625b12e0df63c960c837e22e8e391ab451ec522c60d83e56fc6`)

## Components

- `cli:@lenso/cli@0.2.13` — `sha256:95df550fbe9a4b1321538ff0a5fb17628ab388b61204ad7930fc4e6f1cfcea80`
- `runtime:lenso-autonomous-service@0.1.10` — `sha256:2a91ac572f68cab2d046b2a93d5e04c1dc81cf2b734102884d2872b39cd435e7`
- `runtime:lenso-service@0.1.14` — `sha256:f39f8cb2be59f25af7eb265be2f09afbaac3d570afdae36ff752a3ccf1c7c42a`
- `contracts:lenso-contracts@0.3.15` — `sha256:f93d2b9938d47be11ade3fb9675e295b02db1313dd82afbfbfaff5d41c03df19`
- `provider:lenso-service-provider-v1@1` — `sha256:5cecee2372a80ae8bcec57cd43c1bb19d73d5a3621fa6092f0850028c086bb00`
- `operator:lenso-operator@0.1.0` — `sha256:eda3f717c993979b0cb05dfa7528dd79c0ccf52cbb4a96b3a4178806cf7dd7f4`
- `runtime_console:@lenso/runtime-console@0.1.2` — `sha256:12ad49585fb48d1bfb1958ef1f37a3b73fe65fbabdffd8cde1abcb541b5f859c`

## Manifest and state formats

- `Provider`: `lenso.service.v1`
- `Service`: `lenso.service.v2`
- `System`: `lenso.system.v1`
- `System`: `lenso.system.v2`
- State: `service-store.v1`

## Supported combinations

- `m6-ga-1`: `GeneralAvailability`, state `service-store.v1`, components `cli:@lenso/cli@0.2.13`, `contracts:lenso-contracts@0.3.15`, `operator:lenso-operator@0.1.0`, `provider:lenso-service-provider-v1@1`, `runtime:lenso-autonomous-service@0.1.10`, `runtime:lenso-service@0.1.14`, `runtime_console:@lenso/runtime-console@0.1.2`

## Upgrade and skew edges

- `system-v1-v2`: `lenso.system.v1` -> `lenso.system.v2`; rollback safe `true`; mixed versions ``

Unknown combinations are not inferred compatible from semantic-version proximity.
