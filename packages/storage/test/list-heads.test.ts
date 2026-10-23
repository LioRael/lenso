import { expect, test } from "bun:test";
import type { ObjectMetadata } from "../src/index";
import { headListedObjects } from "../src/list-heads";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((accept, fail) => {
    resolve = accept;
    reject = fail;
  });
  return { promise, resolve, reject };
}

const metadata = (key: string): ObjectMetadata => ({
  key,
  size: key.length,
  contentType: "application/octet-stream",
});

test("a malformed provider list is not treated as an empty page", async () => {
  const malformed = {} as unknown as readonly { Key?: string }[];
  await expect(headListedObjects(malformed, async (key) => metadata(key))).rejects.toBeInstanceOf(
    TypeError,
  );
});

test("list HEADs overlap with bounded fan-out and retain provider order", async () => {
  const first = deferred<ObjectMetadata | null>();
  const second = deferred<ObjectMetadata | null>();
  const lastStarted = deferred<void>();
  const started: string[] = [];
  let active = 0;
  let peak = 0;
  const listing = headListedObjects(
    [{ Key: "a" }, { Key: "b" }, {}, { Key: "" }, { Key: "missing" }, { Key: "c" }],
    (key) => {
      started.push(key);
      active++;
      peak = Math.max(peak, active);
      if (key === "c") lastStarted.resolve();
      const result =
        key === "a"
          ? first.promise
          : key === "b"
            ? second.promise
            : Promise.resolve(key === "missing" ? null : metadata(key));
      return result.finally(() => active--);
    },
  );
  try {
    expect(started).toEqual(["a", "b"]);
    second.resolve(metadata("b"));
    await lastStarted.promise;
    first.resolve(metadata("a"));
    expect(await listing).toEqual([metadata("a"), metadata("b"), metadata("c")]);
    expect(started).toEqual(["a", "b", "missing", "c"]);
    expect(peak).toBe(2);
    expect(active).toBe(0);
  } finally {
    first.resolve(null);
    second.resolve(null);
  }
});

test("a failed HEAD stops queued reads and waits for the earlier in-flight result", async () => {
  const first = deferred<ObjectMetadata | null>();
  const second = deferred<ObjectMetadata | null>();
  const started: string[] = [];
  const firstFailure = new Error("first key failed");
  const secondFailure = new Error("second key failed");
  let settled = false;
  const listing = headListedObjects([{ Key: "a" }, { Key: "b" }, { Key: "c" }], (key) => {
    started.push(key);
    return key === "a" ? first.promise : second.promise;
  }).then(
    (result) => {
      settled = true;
      return result;
    },
    (error: unknown) => {
      settled = true;
      return error;
    },
  );
  try {
    second.reject(secondFailure);
    await second.promise.catch(() => {});
    expect(settled).toBe(false);
    expect(started).toEqual(["a", "b"]);
    first.reject(firstFailure);
    expect(await listing).toBe(firstFailure);
    expect(started).toEqual(["a", "b"]);
  } finally {
    first.resolve(null);
    second.resolve(null);
  }
});

test("cancellation rejects after started HEADs settle without issuing queued reads", async () => {
  for (const count of [2, 5]) {
    const release = deferred<ObjectMetadata | null>();
    const controller = new AbortController();
    const started: string[] = [];
    const reason = new Error("cancelled listing");
    const listing = headListedObjects(
      Array.from({ length: count }, (_, i) => ({ Key: `key-${i}` })),
      (key) => {
        started.push(key);
        return release.promise;
      },
      controller.signal,
    ).catch((error: unknown) => error);
    controller.abort(reason);
    release.resolve(metadata("ignored"));
    expect(await listing).toBe(reason);
    expect(started).toEqual(["key-0", "key-1"]);
  }
});
