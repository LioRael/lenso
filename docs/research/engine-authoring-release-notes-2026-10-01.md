# Configurable authoring release notes

Multiple source-declared Plugins can share a Cargo package. Optional Engine Web
and Contracts processors provide configurable discovery, compilation, output
selection and diagnostics using existing Engine plan/process APIs. Web runtime
remains in WebHost; official contracts generation supports locked materialized
dependencies, consumer-local outputs, compatibility and freshness checks.
Custom WebHost authority can be exported without instantiating facilities.

Authoring public struct additions require the pre-1.0 minor version; the CLI
reexport follows that compatibility boundary. Exact dependency pins are updated
as one compatible cohort.

- lenso 0.5.29
- lenso-agent-tool-cli-plugin 0.1.2
- lenso-capability-configuration-source 0.1.2
- lenso-capability-http-client 0.3.4
- lenso-capability-http-endpoint 0.3.7
- lenso-capability-http-stream-endpoint 0.1.4
- lenso-capability-websocket-endpoint 0.1.4
- lenso-cli 0.7.0
- lenso-contract-codegen 0.10.2
- lenso-engine-app 0.3.3
- lenso-engine-authoring 0.3.0
- lenso-engine-contracts 0.1.0
- lenso-engine-host 0.2.4
- lenso-engine-runtime 0.2.2
- lenso-engine-web 0.1.0
- lenso-host-runtime 0.1.11
- lenso-http-egress-plugin 0.3.7
- lenso-native-adapter 0.3.20
- lenso-native-adapter-macros 0.2.9
- lenso-openapi-plugin 0.2.7
- lenso-web-host 0.2.6
- lenso-web-ingress-plugin 0.4.11
