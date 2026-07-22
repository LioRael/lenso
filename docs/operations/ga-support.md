# Lenso GA Support Manifest

- Protocol: `lenso.ga-support-manifest.v1`
- Manifest ID: `ga-support:19c68b5b926e6292`
- Manifest digest: `sha256:19c68b5b926e629242f51ced34da37bf5eb0220452ce875e52e0b14d3dd624b7`
- Status: `Candidate`
- Documentation: `m6-candidate` (`sha256:8516a631e4279c907c19bd033cb3e3c918fae460362a143b58b2a986b99ee660`)

## Components

- `cli:@lenso/cli@0.1.30` — `sha256:1a9e9bc2e7d7ee80fe7a97993fbbd332d1d6bca899be90bb215548788e45fed2`
- `runtime:lenso-autonomous-service@0.1.0` — `sha256:2afec83dc5afa244582aa392af475e4d5de6228e455efe548bd224e5e1dabe8a`
- `runtime:lenso-service@0.1.4` — `sha256:fc323e176ef96c86b13201741fd318f43b9a16b931a48701e99ccc9e6ef5ace4`
- `contracts:lenso-contracts@0.3.5` — `sha256:0e6626f863b457a3742d353c613f9d2d26c4c786574246a346bb73e7e87a7015`
- `provider:lenso-service-provider-v1@1` — `sha256:5cecee2372a80ae8bcec57cd43c1bb19d73d5a3621fa6092f0850028c086bb00`
- `operator:lenso-operator@0.1.0` — `sha256:eda3f717c993979b0cb05dfa7528dd79c0ccf52cbb4a96b3a4178806cf7dd7f4`
- `runtime_console:@lenso/runtime-console@0.1.1` — `sha256:514c7edd33302f55985aaba08f3217fd2bd52ee65e2340b8f7ec5d6f4fb6944f`

## Supported combinations

- `m6-candidate-1`: `Candidate`, state `service-store.v1`, components `cli:@lenso/cli@0.1.30`, `contracts:lenso-contracts@0.3.5`, `operator:lenso-operator@0.1.0`, `provider:lenso-service-provider-v1@1`, `runtime:lenso-autonomous-service@0.1.0`, `runtime:lenso-service@0.1.4`, `runtime_console:@lenso/runtime-console@0.1.1`

Unknown combinations are not inferred compatible from semantic-version proximity.
