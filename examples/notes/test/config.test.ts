import { expect, test } from "bun:test";
import { AuthConfigurationError } from "@lenso/auth";
import { definePlugin, startApp } from "@lenso/core";
import { resolveConfig, valuesSource } from "@lenso/core/config";
import { envSource } from "@lenso/core/config/env";
import { createNotesAuthPlugin, notesAuthConfig, parseNotesPrincipals } from "../src/auth";
import { createApplicationAuth } from "../src/application-auth";
import { notesListenerConfig } from "../src/server";

const principals = [{ subjectId: " A ", key: "ab".repeat(32) }];

test("Notes array and JSON inputs share normalization and session defaults", async () => {
  for (const input of [principals, JSON.stringify(principals)]) {
    const snapshot = await resolveConfig("notes-auth", {
      contract: notesAuthConfig,
      sources: [valuesSource({ principals: input })],
    });
    expect(snapshot.value).toEqual({
      principals: [{ subjectId: "A", key: principals[0]!.key }],
      lifetime: { idle: 3_600_000, absolute: 86_400_000, renewAfter: 60_000 },
    });
  }
  expect(parseNotesPrincipals(JSON.stringify(principals))[0]!.subjectId).toBe("A");
  expect(() => parseNotesPrincipals("private malformed input")).toThrow(AuthConfigurationError);
});

test("invalid Notes configuration starts neither database nor session store", async () => {
  for (const input of [
    { principals: "not JSON" },
    { principals, idle: 100, absolute: 50, renewAfter: 1 },
    { principals, renewAfter: 0 },
    { principals: [{ subjectId: "A", key: "bad-key" }] },
  ]) {
    let databaseStarts = 0;
    let storeStarts = 0;
    const database = definePlugin({
      id: "notes-db",
      setup() {
        databaseStarts++;
        return {};
      },
    });
    const authentication = createNotesAuthPlugin({
      database,
      store() {
        storeStarts++;
        throw new Error("must not acquire session store");
      },
      principals: { sources: [valuesSource(input)] },
    });
    await expect(startApp({ plugins: [database, authentication] })).rejects.toBeDefined();
    expect(databaseStarts).toBe(0);
    expect(storeStarts).toBe(0);
  }
});

test("application Auth supplier is deferred and read once before database setup", async () => {
  let reads = 0;
  const database = definePlugin({
    id: "notes-db",
    setup() {
      expect(reads).toBe(1);
      throw new Error("stop before acquiring database");
    },
  });
  const authentication = createApplicationAuth({
    database,
    store() {
      throw new Error("must not acquire store");
    },
    principals() {
      reads++;
      return JSON.stringify(principals);
    },
  });
  expect(reads).toBe(0);
  await expect(startApp({ plugins: [database, authentication] })).rejects.toBeDefined();
  expect(reads).toBe(1);
});

test("listener defaults and strict explicit environment conversion preserve port zero", async () => {
  async function port(value: string | undefined) {
    return (
      await resolveConfig("notes-listener", {
        contract: notesListenerConfig,
        sources: [
          envSource({
            id: "notes-listener-env",
            read: () => value,
            bindings: { port: { name: "LENSO_PORT", type: "number" } },
          }),
        ],
      })
    ).value.port;
  }
  expect(await port(undefined)).toBe(3001);
  expect(await port("0")).toBe(0);
  for (const invalid of ["", "3001junk", "1.5", "-1", "65536"])
    await expect(port(invalid)).rejects.toBeDefined();
});
