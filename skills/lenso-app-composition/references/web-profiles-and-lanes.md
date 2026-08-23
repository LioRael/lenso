# Web profiles and Execution Lanes

Read the section that matches the requested authoring branch. Profiles and
lanes are Plan inputs, not Kernel modes.

## Target Web profile

A Web profile names existing keyed Module Instances:

```json
{
  "profiles": {
    "web": {
      "shell": "app-shell",
      "browser_adapter": "browser-adapter",
      "ui_contributions": ["orders-ui"],
      "additional_modules": ["orders-backend"]
    }
  }
}
```

The referenced Instances still appear in `composition.modules`, with roles:

- `app-shell`: `web_shell`, requiring `many lenso.ui.contribution@1`;
- `browser-adapter`: `browser_adapter`, requiring exactly one
  `lenso.web.shell@1` and mirroring each portable business requirement needed by
  a contribution;
- `orders-ui`: `ui_contribution`, providing
  `lenso.ui.contribution@1` and declaring its business Capability requirement;
  and
- `orders-backend`: ordinary business Module providing that Capability.

Resolve with the installed CLI's profile option. The resulting Plan contains
ordinary Instances/bindings only. Reject a profile whose Shell, Browser
Adapter, contribution roles, or mirrored provider bindings are incomplete or
ambiguous.

## Execution Lanes

Declare every lane before placing an Instance:

```json
{
  "execution_lanes": [{ "id": "api-1" }, { "id": "api-2" }],
  "modules": [
    {
      "key": "orders-a",
      "package": "example.orders",
      "execution_lane": "api-1"
    },
    {
      "key": "orders-b",
      "package": "example.orders",
      "execution_lane": "api-2"
    }
  ]
}
```

Placement creates separate Module Instances and Kernel lanes. It does not
clone one mutable Instance, enable work stealing, move Instances at runtime, or
make a non-transferable Capability cross-lane. Verify the selected Runner can
assemble all lanes and every cross-lane request contract explicitly permits
transfer.

## Completion

The branch is complete when profile expansion/placement is visible in the
canonical Plan, all role and cross-lane constraints pass before boot, and
removing the profile or replica leaves the ordinary base Composition valid.
