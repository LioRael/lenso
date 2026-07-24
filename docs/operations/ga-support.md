# Lenso GA Support Manifest

- Protocol: `lenso.ga-support-manifest.v1`
- Manifest ID: `ga-support:85270ae7dd399530`
- Manifest digest: `sha256:85270ae7dd399530011b45664ecef834d53806eb33c55fa410179bb21dbbfa1e`
- Status: `GeneralAvailability`
- Documentation: `m6-ga` (`sha256:09aeded7e3eb0625b12e0df63c960c837e22e8e391ab451ec522c60d83e56fc6`)

## Components

- `cli:@lenso/cli@0.2.9` — `sha256:7d1dde3e3464cba59824d9df18d5f8a59db40efe5f6383ef0204f0dc70b2d9e2`
- `runtime:lenso-autonomous-service@0.1.10` — `sha256:2a91ac572f68cab2d046b2a93d5e04c1dc81cf2b734102884d2872b39cd435e7`
- `runtime:lenso-service@0.1.14` — `sha256:f39f8cb2be59f25af7eb265be2f09afbaac3d570afdae36ff752a3ccf1c7c42a`
- `contracts:lenso-contracts@0.3.15` — `sha256:f93d2b9938d47be11ade3fb9675e295b02db1313dd82afbfbfaff5d41c03df19`
- `provider:lenso-service-provider-v1@1` — `sha256:5cecee2372a80ae8bcec57cd43c1bb19d73d5a3621fa6092f0850028c086bb00`
- `operator:lenso-operator@0.1.0` — `sha256:eda3f717c993979b0cb05dfa7528dd79c0ccf52cbb4a96b3a4178806cf7dd7f4`
- `runtime_console:@lenso/runtime-console@0.1.1` — `sha256:514c7edd33302f55985aaba08f3217fd2bd52ee65e2340b8f7ec5d6f4fb6944f`

## Manifest and state formats

- `Provider`: `lenso.service.v1`
- `Service`: `lenso.service.v2`
- `System`: `lenso.system.v1`
- `System`: `lenso.system.v2`
- State: `service-store.v1`

## Supported combinations

- `m6-ga-1`: `GeneralAvailability`, state `service-store.v1`, components `cli:@lenso/cli@0.2.9`, `contracts:lenso-contracts@0.3.15`, `operator:lenso-operator@0.1.0`, `provider:lenso-service-provider-v1@1`, `runtime:lenso-autonomous-service@0.1.10`, `runtime:lenso-service@0.1.14`, `runtime_console:@lenso/runtime-console@0.1.1`

## Upgrade and skew edges

- `system-v1-v2`: `lenso.system.v1` -> `lenso.system.v2`; rollback safe `true`; mixed versions ``

Unknown combinations are not inferred compatible from semantic-version proximity.
