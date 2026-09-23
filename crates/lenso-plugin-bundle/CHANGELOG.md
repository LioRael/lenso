# Changelog

## 0.6.0 (unpublished)

- Add Plugin Manifest V6's public model for exact Cargo build inputs alongside
  runtime-loadable artifacts. A Cargo build input requires a new linked Host
  build; it cannot be selected as a runtime artifact.
- Extend structured implementation rejection reasons for Cargo build inputs
  and execution controls. Requested controls fail closed without verified Host
  admission evidence.
- This is a minor release under pre-1.0 SemVer: downstream exhaustive matches
  on public manifest and rejection enums must be updated. This note does not
  record a registry publication or deployed target qualification.
