---
packages:
  "@lenso/service-kit": patch
---

### Fixed

Build the Service Kit public entrypoints before the reviewed publisher seals its
npm archive, and reject any future archive that omits a declared entrypoint.
