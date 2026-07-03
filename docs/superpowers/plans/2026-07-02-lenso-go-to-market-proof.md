# Lenso Go-To-Market Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a small launch-proof asset pack for the App Composer direction: spec, plan, screenshots, and a Remotion launch video.

**Architecture:** Keep product strategy docs in `lenso`, and keep visual assets isolated in `lenso-site`. The Remotion project is a separate marketing package under `lenso-site/marketing/launch-video` so site runtime dependencies stay clean.

**Tech Stack:** Markdown specs/plans, Next.js site screenshots, Vite Runtime Console screenshots, Playwright with system Chrome, Remotion, React, TypeScript.

---

## File Structure

- `lenso/docs/superpowers/specs/2026-07-02-lenso-go-to-market-proof-design.md`: product direction and launch asset spec.
- `lenso/docs/superpowers/plans/2026-07-02-lenso-go-to-market-proof.md`: implementation plan.
- `lenso-site/public/lenso-assets/launch/lenso-home-desktop.png`: homepage launch screenshot.
- `lenso-site/public/lenso-assets/launch/lenso-console-launchpad.png`: Runtime Console Launchpad screenshot.
- `lenso-site/public/lenso-assets/launch/lenso-launch-frame.png`: rendered launch video still frame.
- `lenso-site/public/lenso-assets/launch/lenso-launch.mp4`: rendered 60-second launch video.
- `lenso-site/marketing/launch-video/.gitignore`: keeps local install artifacts out of git.
- `lenso-site/marketing/launch-video/package.json`: isolated Remotion package scripts and dependencies.
- `lenso-site/marketing/launch-video/package-lock.json`: lockfile for reproducible Remotion renders.
- `lenso-site/marketing/launch-video/src/index.ts`: Remotion entrypoint.
- `lenso-site/marketing/launch-video/src/Root.tsx`: Remotion composition registration.
- `lenso-site/marketing/launch-video/src/LensoLaunchVideo.tsx`: launch video scenes and animation.
- `lenso-site/marketing/launch-video/public/screenshots/*.png`: local copies of screenshots for Remotion.

## Task 1: Strategy Spec

**Files:**
- Create: `lenso/docs/superpowers/specs/2026-07-02-lenso-go-to-market-proof-design.md`
- Create: `lenso/docs/superpowers/plans/2026-07-02-lenso-go-to-market-proof.md`

- [x] **Step 1: Write the spec**

Create the design doc with the public promise, target audience, Launchpad/App
Composer entrypoint, Support Desk demo boundary, screenshot requirements, video
requirements, non-goals, and success criteria.

- [x] **Step 2: Write this plan**

Create the implementation plan with concrete file paths, commands, and checks.

## Task 2: Screenshot Assets

**Files:**
- Create: `lenso-site/public/lenso-assets/launch/lenso-home-desktop.png`
- Create: `lenso-site/public/lenso-assets/launch/lenso-console-launchpad.png`

- [x] **Step 1: Start local site**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-site
pnpm dev -- --hostname 127.0.0.1 --port 3000
```

Expected: Next.js serves `http://127.0.0.1:3000`.

- [x] **Step 2: Start Runtime Console**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm dev --host 127.0.0.1 --port 5174
```

Expected: Vite serves `http://127.0.0.1:5174/launchpad` using seeded data.

- [x] **Step 3: Capture screenshots**

Use Playwright with system Chrome:

```js
const { chromium } = await import("playwright");
const browser = await chromium.launch({
  headless: true,
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
});
const page = await browser.newPage({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 1,
});
await page.goto("http://127.0.0.1:3000", { waitUntil: "networkidle" });
await page.screenshot({
  path: "/Users/leosouthey/Projects/framework/lenso-site/public/lenso-assets/launch/lenso-home-desktop.png",
});
await page.goto("http://127.0.0.1:5174/launchpad", { waitUntil: "networkidle" });
await page.screenshot({
  path: "/Users/leosouthey/Projects/framework/lenso-site/public/lenso-assets/launch/lenso-console-launchpad.png",
});
await browser.close();
```

Expected: both PNG files exist and show current product UI.

## Task 3: Remotion Launch Video

**Files:**
- Create: `lenso-site/marketing/launch-video/package.json`
- Create: `lenso-site/marketing/launch-video/package-lock.json`
- Create: `lenso-site/marketing/launch-video/.gitignore`
- Create: `lenso-site/marketing/launch-video/src/index.ts`
- Create: `lenso-site/marketing/launch-video/src/Root.tsx`
- Create: `lenso-site/marketing/launch-video/src/LensoLaunchVideo.tsx`
- Create: `lenso-site/marketing/launch-video/public/screenshots/lenso-home-desktop.png`
- Create: `lenso-site/marketing/launch-video/public/screenshots/lenso-console-launchpad.png`
- Create: `lenso-site/public/lenso-assets/launch/lenso-launch-frame.png`
- Create: `lenso-site/public/lenso-assets/launch/lenso-launch.mp4`

- [x] **Step 1: Create isolated Remotion package**

Add a small Remotion package under `lenso-site/marketing/launch-video`. Do not
add Remotion to the main `lenso-site` package.

- [x] **Step 2: Copy screenshots into Remotion public assets**

Run:

```sh
cp /Users/leosouthey/Projects/framework/lenso-site/public/lenso-assets/launch/lenso-home-desktop.png \
  /Users/leosouthey/Projects/framework/lenso-site/marketing/launch-video/public/screenshots/lenso-home-desktop.png
cp /Users/leosouthey/Projects/framework/lenso-site/public/lenso-assets/launch/lenso-console-launchpad.png \
  /Users/leosouthey/Projects/framework/lenso-site/marketing/launch-video/public/screenshots/lenso-console-launchpad.png
```

- [x] **Step 3: Render the video**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-site/marketing/launch-video
npm install
npm run render
```

Expected: `../../public/lenso-assets/launch/lenso-launch.mp4` exists.

- [x] **Step 4: Render a still frame**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-site/marketing/launch-video
npm run still
```

Expected: `../../public/lenso-assets/launch/lenso-launch-frame.png` exists and
the main text is readable.

## Task 4: Validation

**Files:**
- No source files beyond the assets above.

- [x] **Step 1: Validate site**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-site
pnpm lint
pnpm build
```

Expected: both pass.

- [x] **Step 2: Validate Remotion output**

Run:

```sh
test -s /Users/leosouthey/Projects/framework/lenso-site/public/lenso-assets/launch/lenso-launch.mp4
test -s /Users/leosouthey/Projects/framework/lenso-site/public/lenso-assets/launch/lenso-launch-frame.png
test -s /Users/leosouthey/Projects/framework/lenso-site/public/lenso-assets/launch/lenso-home-desktop.png
test -s /Users/leosouthey/Projects/framework/lenso-site/public/lenso-assets/launch/lenso-console-launchpad.png
```

Expected: all commands exit with status 0.

## Task 5: Commit Boundary

**Files:**
- Stage only files listed in this plan.

- [x] **Step 1: Review diff**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso status --short
git -C /Users/leosouthey/Projects/framework/lenso-site status --short
```

Expected: only strategy docs and launch assets are modified.

- [ ] **Step 2: Commit docs and assets separately**

Leave this step pending until the user asks to publish the docs and assets.

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso add docs/superpowers/specs/2026-07-02-lenso-go-to-market-proof-design.md docs/superpowers/plans/2026-07-02-lenso-go-to-market-proof.md
git -C /Users/leosouthey/Projects/framework/lenso commit -m "docs: plan go-to-market proof"

git -C /Users/leosouthey/Projects/framework/lenso-site add public/lenso-assets/launch marketing/launch-video
git -C /Users/leosouthey/Projects/framework/lenso-site commit -m "docs: add launch video assets"
```

Expected: two focused commits if the user asks to publish them.

## Self-Review

- Spec coverage: covers positioning, blueprints, screenshots, Remotion video,
  and validation.
- Placeholder scan: no placeholder tasks; all asset paths and commands are
  concrete.
- Scope check: product feature work remains in V23/V26 plans; this plan only
  ships the launch proof asset pack.
