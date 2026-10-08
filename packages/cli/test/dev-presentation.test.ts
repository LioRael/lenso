import { expect, test } from "bun:test";
import { createDevPresentation, type DevPresentationOptions } from "../src/dev-presentation";
import sdkPackage from "../../lenso/package.json";

function capture(options: Partial<DevPresentationOptions> = {}) {
  let text = "";
  let time = 100;
  const presentation = createDevPresentation({
    project: "/workspace/greeting",
    environment: {},
    now: () => time,
    ...options,
    output: {
      isTTY: options.output?.isTTY,
      write(value) {
        text += value;
      },
    },
  });
  return { presentation, text: () => text, time: (value: number) => (time = value) };
}

test("reports metadata, actual listener origins and elapsed readiness without guessing URLs", () => {
  const view = capture();
  view.presentation.starting();
  expect(view.text()).toContain(`Lenso ${sdkPackage.version} · Bun ${Bun.version}`);
  expect(view.text()).toContain("Project: greeting");
  expect(view.text()).not.toContain("Ready");
  view.time(342);
  view.presentation.ready({
    urls: ["http://localhost:4321/", "http://user:secret@localhost:4321/private?token=secret"],
    capabilities: ["web", "web", "greeting"],
  });
  expect(view.text()).toContain("URL:     http://localhost:4321/");
  expect(view.text()).toContain("Enabled: web, greeting");
  expect(view.text()).toContain("Ready in 242 ms");
  expect(view.text()).not.toMatch(/secret|token|private|Network/);
  expect(view.text().match(/URL:/g)).toHaveLength(1);
});

test("does not mark failures ready or invent a listener for service-only startup", () => {
  const view = capture();
  view.presentation.ready();
  expect(view.text()).toBe("");
  view.presentation.starting();
  view.presentation.failed();
  view.presentation.ready({ urls: ["http://localhost:3000/"] });
  expect(view.text()).not.toMatch(/Ready|URL:/);
  view.presentation.starting();
  view.time(120);
  view.presentation.ready({ urls: ["http://0.0.0.0:3000/", "http://[::]:3000/", "invalid"] });
  expect(view.text()).toContain("Ready in 20 ms");
  expect(view.text()).not.toContain("URL:");
  expect(view.text().match(/Lenso/g)).toHaveLength(1);
});

test.each([
  [false, {}],
  [true, { NO_COLOR: "" }],
  [true, { CI: "true" }],
  [true, { TERM: "dumb" }],
] as const)("keeps output plain for TTY=%s environment=%j", (isTTY, environment) => {
  const view = capture({ output: { isTTY, write() {} }, environment });
  view.presentation.starting();
  view.presentation.ready();
  view.presentation.failed();
  expect(view.text()).not.toContain("\u001b");
});

test("uses restrained color on TTY and keeps JSON completely silent", () => {
  const tty = capture({ output: { isTTY: true, write() {} } });
  tty.presentation.starting();
  tty.presentation.ready();
  expect(tty.text()).toContain("\u001b[");
  const json = capture({ mode: "json", output: { isTTY: true, write() {} } });
  json.presentation.starting();
  json.presentation.ready({ urls: ["http://localhost:3000/"], capabilities: ["web"] });
  json.presentation.failed();
  expect(json.text()).toBe("");
});
