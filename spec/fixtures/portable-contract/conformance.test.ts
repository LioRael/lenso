import {
  PROFILE_CONTRACT,
  bindProvider,
  decodeCorpusRoundTripRequest,
  decodeCorpusRoundTripResponse,
  decodeRoundTripError,
  decodeRoundTripRequest,
  decodeRoundTripResponse,
  encodeCorpusRoundTripRequest,
  encodeCorpusRoundTripResponse,
  encodeRoundTripError,
  encodeRoundTripRequest,
  encodeRoundTripResponse,
  portableValueProfile,
  type InvocationContext,
  type Provider,
  type RoundTripRequest,
} from "./generated/profile.ts";
import { expect, test } from "bun:test";

const corpus = JSON.parse(
  await Bun.file(new URL("./conformance.json", import.meta.url)).text(),
) as Array<{ name: string; wire: unknown }>;

test("generated TypeScript profile round-trips the shared corpus", () => {
  expect(portableValueProfile.int64).toBe("decimal-string");
  expect(portableValueProfile.uint64).toBe("decimal-string");
  expect(portableValueProfile.bytes).toBe("base64-string");
  expect(portableValueProfile.missingAndNull).toBe("distinct");

  for (const fixture of corpus) {
    const corpusValue = fixture.wire as Record<string, unknown>;
    const typedCorpusValue = { value: corpusValue };
    expect(
      decodeCorpusRoundTripRequest(
        encodeCorpusRoundTripRequest(typedCorpusValue),
      ),
    ).toEqual(typedCorpusValue);
    expect(
      decodeCorpusRoundTripResponse(
        encodeCorpusRoundTripResponse(typedCorpusValue),
      ),
    ).toEqual(typedCorpusValue);

    const opaqueError = {
      code: `future_${fixture.name}`,
      payload: fixture.wire,
    };
    expect(
      decodeRoundTripError(encodeRoundTripError(opaqueError)),
    ).toEqual(opaqueError);
  }

  expect(
    decodeRoundTripResponse(
      encodeRoundTripResponse({ accepted: true, echo: null }),
    ),
  ).toEqual({ accepted: true, echo: null });
  expect(decodeRoundTripError(encodeRoundTripError("rejected"))).toBe("rejected");

  const profileRequest = {
    duration: "PT1.5S",
    local_note: "portable",
    name: "Ada",
    nullable_map: { first: 1, second: null },
    nullable_values: ["one", null],
    optional_note: null,
    payload: "AQI=",
    signed: "-9223372036854775808",
    timestamp: "2026-08-21T12:34:56.123Z",
    unsigned: "18446744073709551615",
    values: [1, 2, 3],
  } as unknown as RoundTripRequest;
  expect(decodeRoundTripRequest(encodeRoundTripRequest(profileRequest))).toEqual(
    profileRequest,
  );
  expect(
    decodeRoundTripError(JSON.stringify({ code: "future", payload: null })),
  ).toEqual({ code: "future", payload: null });
  expect(
    encodeRoundTripError({ code: "future", payload: null }),
  ).toBe('{"code":"future","payload":null}');
  const unknownWithExtra = decodeRoundTripError(
    '{"code":"future","payload":{"reason":"later"},"retry_after_ms":2500}',
  );
  expect(unknownWithExtra).toEqual({
    code: "future",
    payload: { reason: "later" },
    retry_after_ms: 2500,
  });
  expect(encodeRoundTripError(unknownWithExtra)).toBe(
    '{"code":"future","payload":{"reason":"later"},"retry_after_ms":2500}',
  );
  expect(
    encodeRoundTripError(decodeRoundTripError('"future_without_payload"')),
  ).toBe('{"code":"future_without_payload"}');
  expect(() =>
    decodeRoundTripRequest(
      '{"duration":"PT1S","name":"Ada","payload":"AQI=","signed":"0","timestamp":"2026-08-21T00:00:00Z","unsigned":"0","values":[9007199254740992]}',
    ),
  ).toThrow("unsafe number");
  expect(() =>
    decodeRoundTripError(
      '{"code":"future","payload":9007199254740992.5}',
    ),
  ).toThrow("unsafe number");
});

test("generated dependency binding maps Host outcomes into the typed client", async () => {
  const calls: Array<{ operation: string; payload: unknown }> = [];
  expect(PROFILE_CONTRACT.capability_id).toBe("example.profile@1");
  expect(PROFILE_CONTRACT.descriptor.descriptor_version).toBe("1.0.0");
  const client = PROFILE_CONTRACT.createClient(async (operation, _context, payload) => {
    calls.push({ operation, payload });
    const request = payload as { name: string; optional_note?: string | null };
    if (request.name.length === 0) return { kind: "domain", value: "rejected" };
    return {
      kind: "success",
      value: request.optional_note === undefined
        ? { accepted: true }
        : { accepted: true, echo: request.optional_note },
    };
  });
  const request = {
    duration: "PT1.5S",
    name: "Ada",
    payload: "AQI=",
    signed: "-1",
    timestamp: "2026-08-21T12:34:56Z",
    unsigned: "1",
    values: [1],
  } as unknown as RoundTripRequest;

  expect(await client.round_trip(request)).toEqual({
    ok: true,
    value: { accepted: true },
  });
  expect(await client.round_trip({ ...request, name: "" })).toEqual({
    ok: false,
    error: { kind: "domain", error: "rejected" },
  });
  expect(calls.map(({ operation }) => operation)).toEqual([
    "round_trip",
    "round_trip",
  ]);
});

test("generated provider binding dispatches typed request outcomes", async () => {
  const provider: Provider = {
    async corpus_round_trip(_context, request) {
      return { ok: true, value: request };
    },
    async round_trip(_context, request) {
      if (request.name.length === 0) {
        return {
          ok: false,
          error: { kind: "domain", error: "rejected" },
        };
      }
      return {
        ok: true,
        value:
          request.optional_note === undefined
            ? { accepted: true }
            : { accepted: true, echo: request.optional_note },
      };
    },
  };
  const binding = bindProvider(provider);
  const context = {
    requestId: "7",
    cancelled: false,
  } as InvocationContext;
  const request = {
    duration: "PT1.5S",
    name: "Ada",
    payload: "AQI=",
    signed: "-1",
    timestamp: "2026-08-21T12:34:56Z",
    unsigned: "1",
    values: [1],
  } as unknown as RoundTripRequest;

  expect(binding.descriptor).toEqual({
    capability_id: "example.profile@1",
    descriptor_version: "1.0.0",
    operations: ["corpus_round_trip", "round_trip"],
    stream_operations: [],
    event_operations: [],
  });
  expect(await binding.invokeRequest("round_trip", context, request)).toEqual({
    kind: "success",
    value: { accepted: true },
  });
  expect(
    await binding.invokeRequest("round_trip", context, { ...request, name: "" }),
  ).toEqual({ kind: "domain", value: "rejected" });
  expect(await binding.invokeRequest("missing", context, request)).toEqual({
    kind: "runtime",
    failure: { kind: "unknown_operation", operation: "missing" },
  });
});
