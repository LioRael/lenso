import type { ObjectMetadata } from "./index";

export async function headListedObjects(
  entries: readonly { Key?: string }[],
  head: (key: string) => Promise<ObjectMetadata | null>,
  signal?: AbortSignal,
): Promise<ObjectMetadata[]> {
  if (!Array.isArray(entries)) throw new TypeError("Invalid provider object list");
  const objects: (ObjectMetadata | null)[] = [];
  let next = 0;
  let failureIndex = entries.length;
  let failure: unknown;
  const read = async () => {
    while (next < entries.length && failureIndex === entries.length) {
      const index = next++;
      try {
        const key = entries[index]!.Key;
        if (!key) continue;
        signal?.throwIfAborted();
        objects[index] = await head(key);
      } catch (error) {
        if (index < failureIndex) {
          failureIndex = index;
          failure = error;
        }
      }
    }
  };
  // Settle both started reads before returning, including on failure or cancellation.
  await Promise.all(Array.from({ length: Math.min(2, entries.length) }, read));
  if (failureIndex !== entries.length) throw failure;
  signal?.throwIfAborted();
  return objects.filter((object): object is ObjectMetadata => object != null);
}
