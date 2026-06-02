# Runtime Console Visual Gap Report

This pass compares the current prototype against the iii console frontend source as a visual reference, without copying implementation code.

## What Still Differs

- The story list still reads slightly too much like a product sidebar: rows are taller, labels use more sans typography, and selected rows need a sharper runtime-workbench state.
- The workbench chrome has a little too much vertical air in headers, toolbars, tabs, and service summaries.
- Inspector sections are close, but tabs and JSON panels need a denser monospace treatment with subtler borders and less card-like padding.
- Waterfall lanes need to stay closer to iii proportions: a compact execution column, 24px lanes, 16px bars, and restrained time-axis labels.
- Flame and heatmap views need tighter cells/bars and higher contrast. Heatmap cells should feel like dense telemetry pixels, not dashboard tiles.
- Buttons and chips remain a little rounded in places; iii leans toward compact square-ish controls with yellow used sparingly.

## Adjustments In This Pass

- Reduce vertical padding and font sizes in story headers, rows, toolbars, tabs, inspector, and summary strip.
- Use monospace more consistently for runtime data, story names, IDs, timestamps, tabs, chips, and inspector fields.
- Make borders thinner visually by preferring `#1d1d1d` over brighter panel borders.
- Tighten workbench proportions to favor the central runtime visualization and a compact inspector.
- Improve selected story and selected execution states with a left yellow rail and low-opacity yellow fill.
- Refine JSON panels into terminal-like collapsible blocks with compact headers.
- Increase heatmap density, reduce gaps, and improve error/slow/active contrast.
- Tighten waterfall and flame graph lanes to better match iii runtime visualization proportions.

## Intentionally Different For Now

- Visualizations remain CSS/div based mock views rather than canvas/resizable production runtime components.
- Panels are fixed-width rather than iii's resizable panel system.
- Some visual reference states still use mocks; direct telemetry storage browsing is out of scope.
- Flow view remains a simple causality sketch, not a full graph layout engine.
