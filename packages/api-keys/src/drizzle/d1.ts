import type { D1Database } from "@cloudflare/workers-types";
import { drizzle } from "drizzle-orm/d1";
import type { ApiKeyStore } from "../store";
import { safeStore } from "./shared";
import { sqliteStore } from "./sqlite";

/**
 * Accept the authoritative binding, not an existing session or replica-capable
 * Drizzle client. A fresh first-primary session makes each operation's initial
 * read authoritative and keeps create's subsequent reads after its write.
 * https://developers.cloudflare.com/d1/worker-api/d1-database/#withsession
 */
export function d1ApiKeyStore(binding: D1Database): ApiKeyStore {
  const operation = () => sqliteStore(drizzle(binding.withSession("first-primary")));
  return safeStore({
    create: (record) => operation().create(record),
    read: (id) => operation().read(id),
    list: (subject, after, limit) => operation().list(subject, after, limit),
    rotate: (input) => operation().rotate(input),
    revoke: (subject, id, now) => operation().revoke(subject, id, now),
  });
}
