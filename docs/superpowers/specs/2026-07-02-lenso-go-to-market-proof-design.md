# Lenso Go-To-Market Proof Design

## Goal

Make Lenso easier to understand, star, and try by focusing the public story on
one concrete promise:

```text
Build Rust business apps that agents can safely change and prove.
```

The near-term proof is a `support-desk` Launchpad/App Composer path with real
Runtime Console evidence, screenshots, and a 60-second Remotion launch video.

## Positioning

Lenso should not compete head-on as a generic Rust web framework, a cloud
deployment platform, or a full AI runtime.

The public wedge is:

- Rust business apps, not raw routing.
- Product blueprints and addons, not blank scaffolds only.
- Agent-safe change rails, not vague AI-native claims.
- Runtime Console proof, not screenshots of generated files alone.

The headline stays compatible with current code:

```text
Lenso = Rust business app framework with modules, Runtime Console, contracts,
and agent-ready checks.
```

The sharper launch line is:

```text
Build Rust business apps that agents can safely change and prove.
```

## Audience

Primary audience:

- Rust developers building SaaS, internal tools, vertical business systems, or
  AI-assisted business workflows.
- Small teams that like Axum and Postgres but do not want to rebuild module
  manifests, admin APIs, operator views, contract checks, and agent handoff
  context for every app.

Secondary audience:

- AI-assisted engineering users who need generated backend changes to come with
  reviewable plans and proof.

## Product Direction

### 1. Launchpad and App Composer become the public entrypoint

The first story should be:

```sh
lenso app create support-desk --blueprint support-desk
lenso app add support-sla
lenso app compose --addon customer-profile --write-plan
lenso app verify --write-proof
lenso agent task --from-app-plan
```

`lenso host init` remains valid, but it is no longer the first public story.
Blank host scaffolding is for users who already know they want a low-level host.

### 2. Support Desk is the single launch demo

The first public demo should be one concrete app:

- `support-desk` blueprint
- `support-sla` addon
- `customer-profile` addon
- App Change Plan
- App Proof
- Runtime Console Launchpad/App Lifecycle screen
- Agent task handoff

This avoids a generic feature tour.

### 3. Screenshots and video prove the value

The launch asset pack must include:

- homepage hero screenshot
- Runtime Console Launchpad/App Lifecycle screenshot
- Remotion-rendered 60-second launch video
- one README/launch-post-safe script that narrates the flow

The video should show product proof, not abstract architecture.

### 4. Agent rails are first-class

Lenso should describe agent support through concrete rails:

- public skills
- manifests
- contracts
- App Change Plans
- App Proof
- Runtime Console evidence

Avoid broad AI runtime claims until there is an actual runtime product surface.

### 5. Blueprints stay curated before marketplace

The next useful blueprints are:

- `support-desk`
- `customer-ops`
- `billing-ops`

Do not build a marketplace until there are at least three high-quality built-in
blueprints with verified examples.

## Launch Asset Requirements

### Screenshots

Create screenshots from real local pages:

- `lenso-home-desktop.png`: Lenso homepage first viewport.
- `lenso-console-launchpad.png`: Runtime Console Launchpad/App Lifecycle view.

Screenshots live in:

```text
lenso-site/public/lenso-assets/launch/
```

### Video

Create a Remotion project in:

```text
lenso-site/marketing/launch-video/
```

The rendered artifact lives in:

```text
lenso-site/public/lenso-assets/launch/lenso-launch.mp4
```

The video should be 60 seconds, 1920x1080, 30fps, with five scenes:

1. Problem: Axum gives routing, but business apps need rails.
2. Create: `lenso app create support-desk --blueprint support-desk`.
3. Compose: add `support-sla` and `customer-profile`.
4. Prove: Runtime Console, App Change Plan, App Proof.
5. Agent handoff: agents get context and proof instead of guessing.

## Non-Goals

This work does not add:

- new runtime framework features
- a cloud platform
- marketplace UX
- hosted video service integration
- voiceover
- paid template packaging
- new Runtime Console product pages

The video can use generated motion graphics plus real screenshots. It does not
need live screen recording.

## Success Criteria

- A new contributor can understand the direction from the spec.
- A worker can implement the launch asset pack from the plan.
- The screenshot assets are generated from current local pages.
- The Remotion project renders a real MP4.
- The site still passes `pnpm lint` and `pnpm build`.
- The launch story stays focused on `support-desk`, not a broad framework tour.
