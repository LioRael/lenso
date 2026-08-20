import {
  decodeGreetError,
  decodeGreetRequest,
  encodeGreetError,
  encodeGreetRequest,
  portableValueProfile,
} from "../../crates/lenso-capability-greeting/generated/bindings.ts";
import {
  decodeRoundTripError,
  decodeRoundTripRequest,
  encodeRoundTripError,
  encodeRoundTripRequest,
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
    const encoded = JSON.stringify(fixture.wire);
    const decoded = JSON.parse(encoded);
    expect(decoded).toEqual(fixture.wire);
    expect(JSON.stringify(decoded)).toBe(encoded);

    const opaqueError = {
      code: `future_${fixture.name}`,
      payload: fixture.wire,
    };
    expect(
      decodeRoundTripError(encodeRoundTripError(opaqueError)),
    ).toEqual(opaqueError);
  }

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
  expect(
    encodeRoundTripError(decodeRoundTripError('"future_without_payload"')),
  ).toBe('{"code":"future_without_payload"}');
  expect(() =>
    decodeRoundTripRequest(
      '{"duration":"PT1S","name":"Ada","payload":"AQI=","signed":"0","timestamp":"2026-08-21T00:00:00Z","unsigned":"0","values":[9007199254740992]}',
    ),
  ).toThrow("unsafe number");
});
