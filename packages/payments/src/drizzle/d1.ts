import type { D1Database } from "@cloudflare/workers-types";
import { drizzle } from "drizzle-orm/d1";
import type { PaymentsStore } from "../contracts";
import { PaymentsError } from "../contracts";
import { sqlitePaymentsStore } from "./sqlite";

/**
 * Borrow the raw D1 binding, NOT a D1 session. Queries without Sessions API route
 * to the primary. A long-lived first-primary session is only sequentially consistent,
 * and can miss reservations committed by another worker.
 * CAS and inserts are single conditional INSERT/UPDATE ... RETURNING statements;
 * D1 interactive transactions are deliberately not used.
 */
export function d1PaymentsStore(binding: D1Database): PaymentsStore {
  if (typeof binding.withSession !== "function" || "getBookmark" in binding)
    throw new PaymentsError("invalid-input");
  return sqlitePaymentsStore(drizzle(binding));
}
