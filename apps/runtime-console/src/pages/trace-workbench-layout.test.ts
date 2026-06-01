import { describe, expect, test } from "vitest";

import {
  resizeTraceInspectorLayout,
  resizeServicesPanelHeight,
  resizeServicesPanelLayout,
  traceLayoutDefaults,
} from "./trace-workbench-layout";

describe("trace workbench layout", () => {
  test("resizes the services panel by drag direction", () => {
    expect(
      resizeServicesPanelHeight(traceLayoutDefaults.servicesHeight, -48)
    ).toBe(traceLayoutDefaults.servicesHeight + 48);
    expect(
      resizeServicesPanelHeight(traceLayoutDefaults.servicesHeight, 24)
    ).toBe(traceLayoutDefaults.servicesHeight - 24);
  });

  test("clamps the services panel to its supported height range", () => {
    expect(resizeServicesPanelHeight(150, 80)).toBe(112);
    expect(resizeServicesPanelHeight(300, -120)).toBe(360);
  });

  test("opens collapsed services when dragged upward", () => {
    const layout = resizeServicesPanelLayout({
      currentHeight: traceLayoutDefaults.servicesHeight,
      deltaY: -8,
      expanded: false,
    });

    expect(layout.expanded).toBe(true);
    expect(layout.height).toBe(traceLayoutDefaults.servicesHeight);
  });

  test("collapses services when dragged below the minimum height", () => {
    const layout = resizeServicesPanelLayout({
      currentHeight: 112,
      deltaY: 8,
      expanded: true,
    });

    expect(layout.expanded).toBe(false);
    expect(layout.height).toBe(112);
  });

  test("keeps services expanded while resizing above the minimum height", () => {
    const layout = resizeServicesPanelLayout({
      currentHeight: 180,
      deltaY: 24,
      expanded: true,
    });

    expect(layout.expanded).toBe(true);
    expect(layout.height).toBe(156);
  });

  test("closes the trace inspector when dragged below the minimum width", () => {
    const layout = resizeTraceInspectorLayout({
      currentWidth: 280,
      deltaX: 8,
    });

    expect(layout.open).toBe(false);
    expect(layout.width).toBe(280);
  });

  test("keeps the trace inspector open while resizing above the minimum width", () => {
    const layout = resizeTraceInspectorLayout({
      currentWidth: 320,
      deltaX: 24,
    });

    expect(layout.open).toBe(true);
    expect(layout.width).toBe(296);
  });
});
