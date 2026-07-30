---
packages:
  lenso-service:
    type: patch
  lenso:
    type: patch
---

### Features

Publish the signed System Plane enrollment, observability, and operations
contracts, then expose the Console and System Plane authoring boundaries through
the public `lenso` facade.

### Security

Require consumers to verify the Console-signed Offer and Service-signed Receipt
as one bilateral enrollment exchange before accepting registry evidence.
