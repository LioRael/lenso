import { createHmac, timingSafeEqual } from 'node:crypto';

export interface SigningKey {
  readonly id: string;
  readonly secret: Uint8Array;
}

const identifier = /^[A-Za-z0-9_-]{1,128}$/;
const timestampPattern = /^(0|[1-9][0-9]{0,15})$/;

function validKey(key: SigningKey): boolean {
  return identifier.test(key.id) && key.secret instanceof Uint8Array && key.secret.byteLength > 0;
}

function digest(body: Uint8Array, eventId: string, timestamp: string, key: SigningKey): Buffer {
  // The newline-delimited prefix is unambiguous because event IDs cannot contain newlines.
  return createHmac('sha256', key.secret).update(`${timestamp}\n${eventId}\n`).update(body).digest();
}

export function signWebhook(
  body: Uint8Array,
  eventId: string,
  timestamp: number,
  key: SigningKey,
): Readonly<Record<string, string>> {
  if (!(body instanceof Uint8Array) || !identifier.test(eventId) ||
      !Number.isSafeInteger(timestamp) || timestamp < 0 || !validKey(key)) {
    throw new Error('Invalid webhook signing input');
  }
  const time = String(timestamp);
  return Object.freeze({
    'x-lenso-event-id': eventId,
    'x-lenso-timestamp': time,
    'x-lenso-signature': `v1;kid=${key.id};mac=${digest(body, eventId, time, key).toString('hex')}`,
  });
}

export function verifyWebhook(input: {
  body: Uint8Array;
  eventId: string;
  timestamp: string;
  signature: string;
  keys: readonly SigningKey[];
  now: number;
  toleranceSeconds?: number;
}): boolean {
  const tolerance = input.toleranceSeconds ?? 300;
  if (!(input.body instanceof Uint8Array) || !identifier.test(input.eventId) || input.timestamp.length > 16 ||
      !timestampPattern.test(input.timestamp) || input.signature.length > 220 ||
      !Number.isFinite(input.now) || !Number.isFinite(tolerance) || tolerance < 0 ||
      input.keys.length > 64) return false;
  const timestamp = Number(input.timestamp);
  if (!Number.isSafeInteger(timestamp) || Math.abs(input.now - timestamp) > tolerance) return false;
  const match = /^v1;kid=([A-Za-z0-9_-]{1,128});mac=([a-f0-9]{64})$/.exec(input.signature);
  if (!match) return false;
  const received = Buffer.from(match[2]!, 'hex');
  let verified = false;
  for (const key of input.keys) {
    if (validKey(key) && key.id === match[1]) {
      verified = timingSafeEqual(received, digest(input.body, input.eventId, input.timestamp, key)) || verified;
    }
  }
  return verified;
}
