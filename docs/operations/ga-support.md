# Lenso GA Support Manifest

> **Legacy v0.3.x operations guide:** This page applies to the maintained
> Service-oriented release line on `main` and is retained as a migration
> reference. See the [documentation map](../README.md).

- Protocol: `lenso.ga-support-manifest.v1`
- Manifest ID: `ga-support:948dc60f7dc6bf25`
- Manifest digest: `sha256:948dc60f7dc6bf252b385b3ee74d1ffe8e39effe33d24bc635a9309ecff536d7`
- Status: `GeneralAvailability`
- Documentation: `m6-ga` (`sha256:f372c19d69f5cb951dd2a795303073ad1113f767ceed1623c4ab181d2d284c6b`)

## Components

- `cli:@lenso/cli@0.2.13` — `sha256:95df550fbe9a4b1321538ff0a5fb17628ab388b61204ad7930fc4e6f1cfcea80`
- `runtime:lenso-autonomous-service@0.1.11` — `sha256:480f1595d9e74e6a0fe2bcd23c4d3a32afa280efe943507a054b75cbbdedf9c5`
- `runtime:lenso-service@0.1.15` — `sha256:61c9188bf346e92e9b4311907a210863b563bbc312ce7ad278719d296c8d52ae`
- `contracts:lenso-contracts@0.3.16` — `sha256:59495c0f65fd92cccd8d054a2cdd476f55ea0afd82dfd0f1ccaec96827f11d4a`
- `provider:lenso-service-provider-v1@1` — `sha256:5cecee2372a80ae8bcec57cd43c1bb19d73d5a3621fa6092f0850028c086bb00`
- `operator:lenso-operator@0.1.0` — `sha256:eda3f717c993979b0cb05dfa7528dd79c0ccf52cbb4a96b3a4178806cf7dd7f4`
- `runtime_console:lenso-console@0.1.2` — `sha256:62a69b216c80fb0b3abe65108fe812b598f0f6375c4a1737c8df7942d0b25b58`

## Manifest and state formats

- `Provider`: `lenso.service.v1`
- `Service`: `lenso.service.v2`
- `System`: `lenso.system.v1`
- `System`: `lenso.system.v2`
- State: `service-store.v1`

## Supported combinations

- `m6-ga-1`: `GeneralAvailability`, state `service-store.v1`, components `cli:@lenso/cli@0.2.13`, `contracts:lenso-contracts@0.3.16`, `operator:lenso-operator@0.1.0`, `provider:lenso-service-provider-v1@1`, `runtime:lenso-autonomous-service@0.1.11`, `runtime:lenso-service@0.1.15`, `runtime_console:lenso-console@0.1.2`

## Upgrade and skew edges

- `system-v1-v2`: `lenso.system.v1` -> `lenso.system.v2`; rollback safe `true`; mixed versions ``

Unknown combinations are not inferred compatible from semantic-version proximity.
