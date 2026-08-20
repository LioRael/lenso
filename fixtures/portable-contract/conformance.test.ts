import {
  decodeGreetError,
  decodeGreetRequest,
  encodeGreetError,
  encodeGreetRequest,
  portableValueProfile,
} from "../../crates/lenso-capability-greeting/generated/bindings.ts";
import {
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
    expect(
      decodeCorpusRoundTripRequest(encodeCorpusRoundTripRequest(corpusValue)),
    ).toEqual(corpusValue);
    expect(
      decodeCorpusRoundTripResponse(encodeCorpusRoundTripResponse(corpusValue)),
    ).toEqual(corpusValue);

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

  expect(decodeGreetRequest(encodeGreetRequest({ name: "Ada" }))).toEqual({
    name: "Ada",
  });
  const unknown = decodeGreetError(
    JSON.stringify({ code: "future_variant", payload: { retry_after_ms: 2500 } }),
  );
  expect(unknown).toEqual({
    code: "future_variant",
    payload: { retry_after_ms: 2500 },
  });
  expect(encodeGreetError(unknown)).toBe(
    '{"code":"future_variant","payload":{"retry_after_ms":2500}}',
  );

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
  };
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
