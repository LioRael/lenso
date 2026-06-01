# iii Console Gap Analysis

This document studies the public iii console frontend as a design and architecture reference only. It avoids copying iii source code, assets, product copy, logos, exact component implementations, or proprietary strings.

## iii Source Findings

- Tech stack: React 19, TypeScript, Vite, Tailwind CSS v4, TanStack Router, TanStack Query, Radix UI primitives, lucide-react, class-variance-authority, clsx, tailwind-merge, cmdk, zod, dagre, @xyflow/react, Vitest, and Biome.
- Folder structure: feature routes live under `src/routes`; trace-specific UI lives under `src/components/traces`; reusable controls live under `src/components/ui`; API domains live under `src/api`; trace transforms and color helpers live under `src/lib`; route generation is committed as `routeTree.gen.ts`.
- Styling approach: Tailwind utility classes backed by CSS custom properties. The design system defines black surfaces, subtle borders, a restrained yellow accent, semantic status colors, compact radii, and separate sans/mono roles.
- Trace/workbench components: the trace route composes a list/detail workbench with filters, trace groups, selectable trace visualization, optional span inspector, service breakdown, waterfall, flame graph, map, and flow views.
- Reusable UI primitives: button, badge, card/table fragments, tabs, dialog, command palette, tooltip, skeleton, pagination, empty state, search bar, and JSON viewer.
- Color/token strategy: mostly grayscale surfaces with a high-contrast accent used for active states and warnings. Service/status colors appear in visualization only when they carry data meaning.
- Layout patterns: dense headers, thin separators, fixed or resizable side panels, a central visualization area, compact tabs, and bottom summary strips. Data pages favor workbench grids over marketing-style cards.
- Interaction details: searchable and filterable trace lists, selected-row rails, paused live updates, polling via TanStack Query, resizable split panes, hover/selection states, compact toolbar controls, span navigation, and inspector tabs.

## What iii Does Better

- It has a more mature trace workbench with live update controls, filters, pagination, resizable panels, and span-level detail transitions.
- Its trace views have stronger visual grammar: waterfall rows, flame rectangles, service maps, flow layouts, toolbars, and service summaries each have distinct information roles.
- It uses monospace data typography consistently for IDs, durations, logs, JSON, timestamps, and operational labels.
- Its surfaces are darker and flatter, with subtle borders and less decorative depth.
- Its inspector is more operational: span info, attributes, logs, errors, links/baggage, and navigation are separated into focused tabs.

## What Runtime Console Currently Lacks

- Runtime Console is still mock-only and does not have real polling, trace tree loading, server filtering, or backend-driven pagination.
- Panels are fixed rather than user-resizable.
- Trace filtering is intentionally lightweight and local.
- Visualization components are simplified React/CSS views rather than full canvas/SVG graph systems.
- The inspector has the requested tabs, but the data model is smaller than a production OpenTelemetry-style span payload.
- Flow controls are visual affordances only; zoom, fit, and pan are not wired yet.

## What We Will Emulate Conceptually

- A dense three-column runtime workbench: trace list, central visualization, inspector.
- Terminal-like black surfaces, thin separators, compact square-ish controls, and restrained accent color.
- Monospace-first runtime data: trace IDs, service names, timestamps, status, JSON, logs, and durations.
- Sharp selected states using a narrow accent rail and low-opacity active fill.
- Compact tab strips for visualization modes and inspector sections.
- Distinct trace visual grammars: horizontal waterfall lanes, stacked flame bars, prominent telemetry heatmap cells, and a service/span flow canvas.
- A bottom service summary strip that keeps service cost and error signal visible while inspecting spans.

## What We Will Intentionally Avoid Copying

- iii source code, component implementations, exact class strings, assets, logos, and branded wording.
- iii's exact route architecture, API contracts, trace transforms, polling behavior, and backend integration.
- Exact visual text from iii trace controls or help labels.
- iii's resizable panel implementation.
- iii's public assets and product identity.
- Any behavior requiring non-mock data before Runtime Console has a backend contract.

## Current Calibration Decisions

- Keep Tailwind CSS and the existing Base UI/TanStack Query/ky foundations.
- Stay mock-mode only.
- Use kebab-case component filenames.
- Prefer small targeted visual refinements over wholesale rewrites.
- Make heatmap the default trace visualization so the required heatmap is prominent.
- Keep the design industrial and utilitarian, but make Runtime Console distinct through its own labels, service colors, and simplified mock workbench behavior.
