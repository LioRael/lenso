import { describe, expect, test } from "vitest";

import {
  clampFlowZoom,
  flowViewDefaults,
  getFitToFrameZoom,
  getFrameScrollPosition,
  getNodeBounds,
  getWorkspaceLayout,
  getZoomAroundPoint,
} from "./flow-view-layout";

describe("flow view layout", () => {
  test("clamps zoom controls to the supported range", () => {
    expect(clampFlowZoom(0.2)).toBe(flowViewDefaults.minZoom);
    expect(clampFlowZoom(3)).toBe(flowViewDefaults.maxZoom);
  });

  test("fits a canvas inside the visible viewport", () => {
    expect(
      getFitToFrameZoom({
        canvasHeight: 640,
        canvasWidth: 1200,
        viewportHeight: 608,
        viewportWidth: 1028,
      })
    ).toBe(0.75);
  });

  test("keeps the pointer anchored while zooming", () => {
    const result = getZoomAroundPoint({
      currentZoom: 1,
      nextZoom: 1.25,
      pointerX: 240,
      pointerY: 120,
      scrollLeft: 160,
      scrollTop: 80,
    });

    expect(result.zoom).toBe(1.25);
    expect(result.scrollLeft).toBe(260);
    expect(result.scrollTop).toBe(130);
  });

  test("adds viewport-sized margins around the graph for free panning", () => {
    const layout = getWorkspaceLayout({
      canvasHeight: 420,
      canvasWidth: 980,
      viewportHeight: 520,
      viewportWidth: 372,
      zoom: 1,
    });

    expect(layout.marginLeft).toBe(186);
    expect(layout.marginTop).toBe(260);
    expect(layout.workspaceWidth).toBe(1352);
    expect(layout.workspaceHeight).toBe(940);
  });

  test("frames the graph content into the center of the viewport", () => {
    const position = getFrameScrollPosition({
      bounds: { height: 72, width: 240, x: 0, y: 0 },
      marginLeft: 186,
      marginTop: 260,
      viewportHeight: 520,
      viewportWidth: 372,
      zoom: 0.55,
    });

    expect(position.scrollLeft).toBe(66);
    expect(position.scrollTop).toBe(20);
  });

  test("centers a single node instead of the empty canvas around it", () => {
    const position = getFrameScrollPosition({
      bounds: { height: 72, width: 240, x: 0, y: 0 },
      marginLeft: 183,
      marginTop: 260,
      viewportHeight: 520,
      viewportWidth: 366,
      zoom: 1,
    });

    expect(position.scrollLeft).toBe(120);
    expect(position.scrollTop).toBe(36);
  });

  test("builds graph bounds from node positions", () => {
    expect(
      getNodeBounds([
        { x: 0, y: 0 },
        { x: 280, y: 92 },
      ])
    ).toEqual({ height: 164, width: 520, x: 0, y: 0 });
  });
});
