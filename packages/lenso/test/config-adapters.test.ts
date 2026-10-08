import { describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { resolveConfig, valuesSource } from "../src/config";
import { envSource } from "../src/config-env";
import { jsonFileSource } from "../src/config-file";

describe("envSource", () => {
  test("reads only explicitly bound names and omits missing values", async () => {
    const requested: string[] = [];
    const source = envSource({
      id: "environment",
      read: (name) => {
        requested.push(name);
        return name === "APP_NAME" ? "demo" : undefined;
      },
      bindings: { name: { name: "APP_NAME" }, absent: { name: "UNBOUND" } },
    });

    expect(await source.read({})).toEqual({ values: { name: "demo" } });
    expect(requested).toEqual(["APP_NAME", "UNBOUND"]);
  });

  test("preserves string empties and omits explicitly omitted empties", async () => {
    const source = envSource({
      id: "environment",
      read: () => "",
      bindings: {
        text: { name: "TEXT" },
        ignored: { name: "IGNORED", empty: "omit" },
      },
    });
    expect(await source.read({})).toEqual({ values: { text: "" } });
  });

  test("strictly converts numbers and booleans", async () => {
    const source = envSource({
      id: "environment",
      read: (name) => ({ COUNT: "1.25e2", ENABLED: "false" })[name],
      bindings: {
        count: { name: "COUNT", type: "number" },
        enabled: { name: "ENABLED", type: "boolean" },
      },
    });
    expect(await source.read({})).toEqual({ values: { count: 125, enabled: false } });

    for (const raw of ["", " 1", "0x10", "Infinity", "1e999"]) {
      const invalid = envSource({
        id: "bad-number",
        read: () => raw,
        bindings: { count: { name: "COUNT", type: "number" } },
      });
      await expect(invalid.read({})).rejects.toMatchObject({
        code: "config-env-invalid",
        path: ["count"],
      });
    }
    for (const raw of ["", "TRUE", "yes", " true"]) {
      const invalid = envSource({
        id: "bad-boolean",
        read: () => raw,
        bindings: { enabled: { name: "ENABLED", type: "boolean" } },
      });
      await expect(invalid.read({})).rejects.toMatchObject({ code: "config-env-invalid" });
    }
  });

  test("does not disclose sensitive values in its descriptor", async () => {
    const source = envSource({
      id: "secret",
      read: () => "do-not-disclose",
      bindings: { token: { name: "API_TOKEN", sensitive: true } },
    });
    expect(source.descriptor).toEqual({
      id: "secret",
      kind: "env",
      location: undefined,
      fields: [{ path: ["token"], env: "API_TOKEN", sensitive: true }],
    });
    expect(JSON.stringify(source.descriptor)).not.toContain("do-not-disclose");
  });

  test("rejects unsafe fields and wraps accessor failures without their cause", async () => {
    const unsafe = envSource({
      id: "unsafe",
      read: () => "false",
      bindings: JSON.parse('{"__proto__":{"name":"UNSAFE"}}'),
    });
    await expect(unsafe.read({})).rejects.toMatchObject({ code: "config-env-invalid" });
    const failed = envSource({
      id: "failed",
      read: () => {
        throw Error("private environment accessor");
      },
      bindings: { credential: { name: "CREDENTIAL", sensitive: true } },
    });
    const error = await failed.read({}).catch((cause: unknown) => cause);
    expect(error).toMatchObject({ code: "config-source-failed", path: ["credential"] });
    expect(String(error)).not.toContain("private");
    expect((error as Error).cause).toBeUndefined();
  });
});

describe("jsonFileSource", () => {
  test("resolves against an explicit root and selects an own subobject", async () => {
    let receivedPath = "";
    const source = jsonFileSource({
      id: "settings",
      root: "/app/config",
      path: "settings.json",
      select: ["application", "settings"],
      readFile: async (path) => {
        receivedPath = path;
        return '{"application":{"settings":{"enabled":true}},"ignored":"secret"}';
      },
    });
    expect(await source.read({})).toEqual({ values: { enabled: true } });
    expect(receivedPath).toBe("/app/config/settings.json");
    expect(JSON.stringify(source.descriptor)).not.toContain("settings.json");
  });

  test("optional suppresses only ENOENT", async () => {
    const required = jsonFileSource({
      id: "required",
      root: "/app",
      path: "missing.json",
      readFile: async () => {
        throw Object.assign(new Error("not found"), { code: "ENOENT" });
      },
    });
    await expect(required.read({})).rejects.toMatchObject({ code: "config-file-missing" });

    const missing = jsonFileSource({
      id: "optional",
      root: "/app",
      path: "missing.json",
      optional: true,
      readFile: async () => {
        throw Object.assign(new Error("not found"), { code: "ENOENT" });
      },
    });
    expect(await missing.read({})).toEqual({ values: {} });

    for (const failure of [
      Object.assign(new Error("denied"), { code: "EACCES" }),
      Object.assign(new Error("bad JSON"), { code: "ENOENT" }),
    ]) {
      const source = jsonFileSource({
        id: "required",
        root: "/app",
        path: "settings.json",
        optional: true,
        readFile: async () => {
          if (failure.message === "bad JSON") return "{";
          throw failure;
        },
      });
      await expect(source.read({})).rejects.toMatchObject({ code: "config-file-invalid" });
    }
  });

  test("requires an absolute root and a plain JSON object", async () => {
    expect(() => jsonFileSource({ id: "bad", root: "relative", path: "x.json" })).toThrow();
    const source = jsonFileSource({
      id: "bad-json",
      root: "/app",
      path: "x.json",
      readFile: async () => "[]",
    });
    await expect(source.read({})).rejects.toMatchObject({ code: "config-file-invalid" });
  });

  test("missing/non-object/unsafe selection fails even on an optional file", async () => {
    for (const text of ['{"application":null}', '{"application":[]}', '{"other":{}}']) {
      const source = jsonFileSource({
        id: "selected",
        root: "/app",
        path: "settings.json",
        select: ["application"],
        optional: true,
        readFile: async () => text,
      });
      await expect(source.read({})).rejects.toMatchObject({ code: "config-file-invalid" });
    }
    expect(() =>
      jsonFileSource({
        id: "unsafe",
        root: "/app",
        path: "x.json",
        select: ["constructor"],
      }),
    ).toThrow();
  });

  test("real file, values and injected env compose in declaration order before one validation", async () => {
    const root = await mkdtemp(join(tmpdir(), "lenso-config-"));
    try {
      await writeFile(
        join(root, "settings.json"),
        '{"application":{"port":3001,"nested":{"file":true},"label":null}}',
      );
      let validations = 0;
      const snapshot = await resolveConfig("composed", {
        contract: {
          schema: {
            "~standard": {
              version: 1,
              vendor: "test",
              async validate(value: unknown) {
                validations++;
                return { value };
              },
            },
          },
        },
        sources: [
          valuesSource({ port: 1, nested: { base: true }, label: "base" }),
          jsonFileSource({ id: "file", root, path: "settings.json", select: ["application"] }),
          envSource({
            id: "env",
            read: (name) => (name === "APP_PORT" ? "0" : undefined),
            bindings: { port: { name: "APP_PORT", type: "number" }, label: { name: "APP_LABEL" } },
          }),
        ],
      });
      expect(snapshot.value).toEqual({ port: 0, nested: { file: true }, label: null });
      expect(validations).toBe(1);
      expect(snapshot.provenance.find((field) => field.path[0] === "port")?.sourceIds).toEqual([
        "values",
        "file",
        "env",
      ]);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
