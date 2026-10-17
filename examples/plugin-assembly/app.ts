import { collectPlugins, defineApp, definePlugin } from "@lenso/core";
import { audience, createAuth, realm, type AuthSource } from "@lenso/auth";
import { createAuthPlugin } from "@lenso/auth/plugin";
import { createLocalStoragePlugin } from "@lenso/storage/local";
import { createMemoryCacheAdapter } from "@lenso/cache/memory";
import { createCachePlugin } from "@lenso/cache/plugin";

/** Declarations only. The caller supplies a trusted verifier and a dedicated storage directory. */
export function createNotesApp<E>(root: string, source: AuthSource<E>) {
  const auth = createAuthPlugin({
    id: "auth",
    setup: () => createAuth(realm("notes-users", source)),
  });
  const storage = createLocalStoragePlugin({ id: "storage", root });
  const adapter = definePlugin({ id: "memory", setup: () => createMemoryCacheAdapter() });
  const cache = createCachePlugin<string>({
    id: "note-cache",
    adapter,
    config: { namespace: "notes" },
    validate: (value): value is string => typeof value === "string",
  });
  const notes = definePlugin({
    id: "notes",
    requires: [auth, storage, cache],
    setup(context) {
      const access = context.get(auth).for(audience("notes"));
      const objects = context.get(storage);
      const content = context.get(cache);
      async function key(evidence: E, owner: string) {
        await access.authorized(
          evidence,
          { owner },
          ({ principal, resource }) => principal.subjectId === resource.owner,
        );
        return `notes/${encodeURIComponent(owner)}.txt`;
      }
      return {
        async put(evidence: E, owner: string, text: string) {
          const objectKey = await key(evidence, owner);
          await objects.put({
            key: objectKey,
            body: new Blob([text]).stream(),
            contentType: "text/plain",
          });
          await content.delete(objectKey);
        },
        async read(evidence: E, owner: string): Promise<string> {
          const objectKey = await key(evidence, owner);
          // Authorization runs on every call, before even a cache hit.
          return content.getOrSet(objectKey, async () =>
            new Response((await objects.get(objectKey)).body).text(),
          );
        },
      };
    },
  });
  return { app: defineApp({ plugins: collectPlugins([notes]) }), notes, auth, storage, cache };
}
