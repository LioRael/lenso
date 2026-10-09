import { randomUUID } from "node:crypto";
import type { Pool, PoolClient, QueryResult, QueryResultRow } from "pg";
import {
  WebhookError,
  type Attempt,
  type Delivery,
  type Endpoint,
  type PageInput,
  type StoredDelivery,
  type Subscription,
  type WebhookRepository,
  type WebhookScope,
} from "./contracts";

type Row = QueryResultRow;
type Paging = Required<Pick<PageInput, "limit">> & PageInput;
type Connection = { query(sql: string, values?: unknown[]): Promise<QueryResult> };

function scopeOf(row: Row): WebhookScope {
  return { tenantId: row.tenant_id, scopeId: row.scope_id };
}

function endpoint(row: Row): Endpoint {
  return {
    id: row.id, scope: scopeOf(row), url: row.url, secretRef: row.secret_ref,
    enabled: row.enabled, revision: row.revision, createdAt: Number(row.created_at),
  };
}

function subscription(row: Row): Subscription {
  return {
    id: row.id, scope: scopeOf(row), endpointId: row.endpoint_id,
    eventType: row.event_type, enabled: row.enabled, createdAt: Number(row.created_at),
  };
}

function delivery(row: Row): Delivery {
  return {
    id: row.id, scope: scopeOf(row), eventId: row.event_id,
    endpointId: row.endpoint_id, subscriptionId: row.subscription_id,
    endpointRevision: row.endpoint_revision, state: row.state,
    attemptCount: row.attempt_count, maxAttempts: row.max_attempts,
    dueAt: Number(row.due_at), generation: row.generation, replayOf: row.replay_of,
    auditIntentId: row.audit_intent_id, createdAt: Number(row.created_at),
    updatedAt: Number(row.updated_at), lastCode: row.last_code,
  };
}

function stored(row: Row): StoredDelivery {
  return {
    ...delivery(row), url: row.url, secretRef: row.secret_ref, body: row.body,
    leaseToken: row.lease_token,
    leaseUntil: row.lease_until === null ? null : Number(row.lease_until),
  };
}

function attempt(row: Row): Attempt {
  return {
    id: row.id, deliveryId: row.delivery_id, number: row.number,
    startedAt: Number(row.started_at),
    finishedAt: row.finished_at === null ? null : Number(row.finished_at),
    code: row.code, status: row.status, keyId: row.key_id,
  };
}

function boundedLimit(limit: number, maximum = 500): number {
  if (!Number.isSafeInteger(limit) || limit < 1) throw new WebhookError("invalid-input");
  return Math.min(limit, maximum);
}

function pageValues(scope: WebhookScope, page: Paging): unknown[] {
  return [
    scope.tenantId, scope.scopeId, page.cursor?.createdAt ?? null,
    page.cursor?.id ?? null, boundedLimit(page.limit),
  ];
}

// Only errors deliberately constructed here are allowed through the persistence boundary.
async function safe<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    if (error instanceof WebhookError &&
      ["invalid-input", "not-found", "conflict", "disabled"].includes(error.code)) throw error;
    throw new WebhookError("storage-failed");
  }
}

/** Borrows the host's pg pool; the host applies migrations and owns its lifetime. */
export function createPostgresWebhookRepository({
  pool,
  schema = "public",
}: { pool: Pool; schema?: string }): WebhookRepository {
  if (schema.length > 63 || !/^[a-z_][a-z0-9_]*$/.test(schema)) {
    throw new WebhookError("invalid-config");
  }
  const qualifier = `"${schema}"`;
  function query(target: Pool | PoolClient, sql: string, values?: unknown[]) {
    return target.query(sql.replaceAll("public.lenso_webhook_", `${qualifier}.lenso_webhook_`), values);
  }
  const database: Connection = { query: (sql, values) => query(pool, sql, values) };
  async function transaction<T>(operation: (client: Connection) => Promise<T>): Promise<T> {
    return safe(async () => {
      const client = await pool.connect();
      let discard = false;
      try {
        await query(client, "BEGIN");
        const result = await operation({ query: (sql, values) => query(client, sql, values) });
        await query(client, "COMMIT");
        return result;
      } catch (error) {
        try {
          await query(client, "ROLLBACK");
        } catch {
          discard = true;
        }
        throw error;
      } finally {
        client.release(discard);
      }
    });
  }

  async function active(client: Connection, row: Row): Promise<"endpoint-disabled" | "unsubscribed" | null> {
    // SHARE, not KEY SHARE: enable-flag updates must serialize with claim/replay.
    const ep = await client.query(
      `SELECT enabled FROM public.lenso_webhook_endpoint
       WHERE tenant_id=$1 AND scope_id=$2 AND id=$3 FOR SHARE`,
      [row.tenant_id, row.scope_id, row.endpoint_id],
    );
    const sub = await client.query(
      `SELECT enabled FROM public.lenso_webhook_subscription
       WHERE tenant_id=$1 AND scope_id=$2 AND id=$3 AND endpoint_id=$4 FOR SHARE`,
      [row.tenant_id, row.scope_id, row.subscription_id, row.endpoint_id],
    );
    if (!ep.rows[0]?.enabled) return "endpoint-disabled";
    if (!sub.rows[0]?.enabled) return "unsubscribed";
    return null;
  }

  return {
    putEndpoint(input, now) {
      return transaction(async (client) => {
        const result = await client.query(
          `INSERT INTO public.lenso_webhook_endpoint
             (id,tenant_id,scope_id,url,secret_ref,enabled,revision,created_at)
           VALUES ($1,$2,$3,$4,$5,$6,1,$7)
           ON CONFLICT (id) DO UPDATE SET url=EXCLUDED.url,secret_ref=EXCLUDED.secret_ref,
             enabled=EXCLUDED.enabled,revision=lenso_webhook_endpoint.revision+1
           WHERE lenso_webhook_endpoint.tenant_id=EXCLUDED.tenant_id
             AND lenso_webhook_endpoint.scope_id=EXCLUDED.scope_id
           RETURNING *`,
          [input.id, input.scope.tenantId, input.scope.scopeId, input.url, input.secretRef, input.enabled, now],
        );
        if (!result.rows[0]) throw new WebhookError("conflict");
        return endpoint(result.rows[0]);
      });
    },
    getEndpoint(scope, id) {
      return safe(async () => {
        const result = await database.query(
          "SELECT * FROM public.lenso_webhook_endpoint WHERE tenant_id=$1 AND scope_id=$2 AND id=$3",
          [scope.tenantId, scope.scopeId, id],
        );
        return result.rows[0] ? endpoint(result.rows[0]) : null;
      });
    },
    listEndpoints(scope, page) {
      return safe(async () => {
        const result = await database.query(
          `SELECT * FROM public.lenso_webhook_endpoint WHERE tenant_id=$1 AND scope_id=$2
           AND ($3::bigint IS NULL OR (created_at,id)>($3,$4::uuid))
           ORDER BY created_at,id LIMIT $5`, pageValues(scope, page),
        );
        return result.rows.map(endpoint);
      });
    },
    putSubscription(input, now) {
      return transaction(async (client) => {
        const ep = await client.query(
          "SELECT id FROM public.lenso_webhook_endpoint WHERE tenant_id=$1 AND scope_id=$2 AND id=$3 FOR SHARE",
          [input.scope.tenantId, input.scope.scopeId, input.endpointId],
        );
        if (!ep.rows[0]) throw new WebhookError("not-found");
        const result = await client.query(
          `INSERT INTO public.lenso_webhook_subscription
             (id,tenant_id,scope_id,endpoint_id,event_type,enabled,created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7)
           ON CONFLICT (id) DO UPDATE SET enabled=EXCLUDED.enabled
           WHERE lenso_webhook_subscription.tenant_id=EXCLUDED.tenant_id
             AND lenso_webhook_subscription.scope_id=EXCLUDED.scope_id
             AND lenso_webhook_subscription.endpoint_id=EXCLUDED.endpoint_id
             AND lenso_webhook_subscription.event_type=EXCLUDED.event_type RETURNING *`,
          [input.id, input.scope.tenantId, input.scope.scopeId, input.endpointId, input.eventType, input.enabled, now],
        );
        if (!result.rows[0]) throw new WebhookError("conflict");
        return subscription(result.rows[0]);
      });
    },
    listSubscriptions(scope, page) {
      return safe(async () => {
        const result = await database.query(
          `SELECT * FROM public.lenso_webhook_subscription WHERE tenant_id=$1 AND scope_id=$2
           AND ($3::bigint IS NULL OR (created_at,id)>($3,$4::uuid))
           ORDER BY created_at,id LIMIT $5`, pageValues(scope, page),
        );
        return result.rows.map(subscription);
      });
    },
    publish(scope, event, body, maxAttempts, now) {
      return transaction(async (client) => {
        const inserted = await client.query(
          `INSERT INTO public.lenso_webhook_event (id,tenant_id,scope_id,event_type,envelope,body,created_at)
           VALUES ($1,$2,$3,$4,$5::json,$6,$7) ON CONFLICT (id) DO NOTHING RETURNING id`,
          [event.id, scope.tenantId, scope.scopeId, event.type, JSON.stringify(event), body, now],
        );
        if (!inserted.rows[0]) throw new WebhookError("conflict");
        // Reject the entire publish above 1000 recipients, rather than silently truncating.
        const matches = await client.query(
          `SELECT s.id AS subscription_id,e.* FROM public.lenso_webhook_subscription s
           JOIN public.lenso_webhook_endpoint e
             ON e.tenant_id=s.tenant_id AND e.scope_id=s.scope_id AND e.id=s.endpoint_id
           WHERE s.tenant_id=$1 AND s.scope_id=$2 AND s.event_type=$3 AND s.enabled AND e.enabled
           ORDER BY s.id LIMIT 1001 FOR SHARE OF s,e`,
          [scope.tenantId, scope.scopeId, event.type],
        );
        if (matches.rows.length > 1000) throw new WebhookError("conflict");
        const deliveries: Delivery[] = [];
        for (const row of matches.rows) {
          const result = await client.query(
            `INSERT INTO public.lenso_webhook_delivery
               (id,tenant_id,scope_id,event_id,endpoint_id,subscription_id,endpoint_revision,
                url,secret_ref,body,state,attempt_count,max_attempts,due_at,generation,created_at,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'pending',0,$11,$12,1,$12,$12) RETURNING *`,
            [randomUUID(), scope.tenantId, scope.scopeId, event.id, row.id, row.subscription_id,
              row.revision, row.url, row.secret_ref, body, maxAttempts, now],
          );
          deliveries.push(delivery(result.rows[0]));
        }
        return deliveries;
      });
    },
    getDelivery(scope, id) {
      return safe(async () => {
        const result = await database.query(
          "SELECT * FROM public.lenso_webhook_delivery WHERE tenant_id=$1 AND scope_id=$2 AND id=$3",
          [scope.tenantId, scope.scopeId, id],
        );
        return result.rows[0] ? delivery(result.rows[0]) : null;
      });
    },
    listDeliveries(scope, page) {
      return safe(async () => {
        const result = await database.query(
          `SELECT * FROM public.lenso_webhook_delivery WHERE tenant_id=$1 AND scope_id=$2
           AND ($3::bigint IS NULL OR (created_at,id)>($3,$4::uuid))
           ORDER BY created_at,id LIMIT $5`, pageValues(scope, page),
        );
        return result.rows.map(delivery);
      });
    },
    listAttempts(scope, deliveryId, page) {
      return safe(async () => {
        const result = await database.query(
          `SELECT * FROM public.lenso_webhook_attempt
           WHERE tenant_id=$1 AND scope_id=$2 AND delivery_id=$6
           AND ($3::bigint IS NULL OR (started_at,id)>($3,$4::uuid))
           ORDER BY started_at,id LIMIT $5`,
          [...pageValues(scope, page), deliveryId],
        );
        return result.rows.map(attempt);
      });
    },
    replay(scope, id, newId, auditIntentId, now) {
      return transaction(async (client) => {
        const original = await client.query(
          `SELECT * FROM public.lenso_webhook_delivery
           WHERE tenant_id=$1 AND scope_id=$2 AND id=$3 FOR UPDATE`,
          [scope.tenantId, scope.scopeId, id],
        );
        const row = original.rows[0];
        if (!row) throw new WebhookError("not-found");
        if (!["succeeded", "failed"].includes(row.state)) throw new WebhookError("conflict");
        if (await active(client, row)) throw new WebhookError("disabled");
        const result = await client.query(
          `INSERT INTO public.lenso_webhook_delivery
             (id,tenant_id,scope_id,event_id,endpoint_id,subscription_id,endpoint_revision,
              url,secret_ref,body,state,attempt_count,max_attempts,due_at,generation,
              replay_of,audit_intent_id,created_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'pending',0,$11,$12,1,$13,$14,$12,$12)
           ON CONFLICT (id) DO NOTHING RETURNING *`,
          [newId, scope.tenantId, scope.scopeId, row.event_id, row.endpoint_id, row.subscription_id,
            row.endpoint_revision, row.url, row.secret_ref, row.body, row.max_attempts, now, id, auditIntentId],
        );
        if (!result.rows[0]) throw new WebhookError("conflict");
        return delivery(result.rows[0]);
      });
    },
    claim(id, generation, token, now, leaseMs) {
      return transaction(async (client) => {
        if (!token || !Number.isSafeInteger(leaseMs) || leaseMs <= 0 ||
          !Number.isSafeInteger(now + leaseMs)) throw new WebhookError("invalid-input");
        const selected = await client.query(
          `SELECT * FROM public.lenso_webhook_delivery WHERE id=$1 AND generation=$2
           AND state IN ('pending','retry') AND due_at<=$3 FOR UPDATE`,
          [id, generation, now],
        );
        const row = selected.rows[0];
        if (!row) return null;
        const disabled = await active(client, row);
        if (disabled || row.attempt_count >= row.max_attempts) {
          await client.query(
            `UPDATE public.lenso_webhook_delivery SET state='failed',last_code=$2,updated_at=$3 WHERE id=$1`,
            [id, disabled ?? row.last_code ?? "lease-expired", now],
          );
          return null;
        }
        const updated = await client.query(
          `UPDATE public.lenso_webhook_delivery SET state='running',attempt_count=attempt_count+1,
           lease_token=$2,lease_until=$3,updated_at=$4 WHERE id=$1 RETURNING *`,
          [id, token, now + leaseMs, now],
        );
        const inserted = await client.query(
          `INSERT INTO public.lenso_webhook_attempt
             (id,tenant_id,scope_id,delivery_id,number,started_at)
           VALUES ($1,$2,$3,$4,$5,$6) RETURNING *`,
          [randomUUID(), row.tenant_id, row.scope_id, id, updated.rows[0].attempt_count, now],
        );
        return { delivery: stored(updated.rows[0]), attempt: attempt(inserted.rows[0]) };
      });
    },
    finish(id, token, result, now) {
      return transaction(async (client) => {
        const selected = await client.query(
          `SELECT * FROM public.lenso_webhook_delivery
           WHERE id=$1 AND state='running' AND lease_token=$2 AND lease_until>$3 FOR UPDATE`,
          [id, token, now],
        );
        const row = selected.rows[0];
        if (!row) return null;
        const retry = result.code !== "success" && result.retryAt !== null && row.attempt_count < row.max_attempts;
        const finished = await client.query(
          `UPDATE public.lenso_webhook_attempt SET finished_at=$3,code=$4,status=$5,key_id=$6
           WHERE delivery_id=$1 AND number=$2 AND finished_at IS NULL`,
          [id, row.attempt_count, now, result.code, result.status, result.keyId],
        );
        if (finished.rowCount !== 1) throw new WebhookError("storage-failed");
        const updated = await client.query(
          `UPDATE public.lenso_webhook_delivery SET state=$2,last_code=$3,
           due_at=$4,generation=generation+$5,lease_token=NULL,lease_until=NULL,updated_at=$6
           WHERE id=$1 RETURNING *`,
          [id, result.code === "success" ? "succeeded" : retry ? "retry" : "failed",
            result.code, retry ? result.retryAt : row.due_at, retry ? 1 : 0, now],
        );
        return delivery(updated.rows[0]);
      });
    },
    recover(now, limit) {
      return transaction(async (client) => {
        const selected = await client.query(
          `SELECT * FROM public.lenso_webhook_delivery
           WHERE (state IN ('pending','retry') AND due_at<=$1)
             OR (state='running' AND lease_until<=$1)
           ORDER BY recovery_checked_at,LEAST(due_at,COALESCE(lease_until,due_at)),id
           LIMIT $2 FOR UPDATE SKIP LOCKED`,
          [now, boundedLimit(limit)],
        );
        const deliveries: Delivery[] = [];
        for (const row of selected.rows) {
          // Rotate the bounded scan even when its oldest records have healthy queued jobs.
          await client.query("UPDATE public.lenso_webhook_delivery SET recovery_checked_at=$2 WHERE id=$1", [row.id, now]);
          if (row.state === "running") {
            const abandoned = await client.query(
              `UPDATE public.lenso_webhook_attempt SET finished_at=$3,code='lease-expired'
               WHERE delivery_id=$1 AND number=$2 AND finished_at IS NULL`,
              [row.id, row.attempt_count, now],
            );
            if (abandoned.rowCount !== 1) throw new WebhookError("storage-failed");
          }
          if (row.state !== "running") {
            deliveries.push(delivery(row));
            continue;
          }
          const exhausted = row.attempt_count >= row.max_attempts;
          const updated = await client.query(
            `UPDATE public.lenso_webhook_delivery SET state=$2,generation=generation+1,
             due_at=$3,lease_token=NULL,lease_until=NULL,updated_at=$3,
             last_code=CASE WHEN state='running' THEN 'lease-expired' ELSE last_code END
             WHERE id=$1 RETURNING *`,
            [row.id, exhausted ? "failed" : row.state === "pending" ? "pending" : "retry", now],
          );
          if (!exhausted) deliveries.push(delivery(updated.rows[0]));
        }
        return deliveries;
      });
    },
    advanceSchedule(id, generation, now) {
      return transaction(async (client) => {
        const updated = await client.query(
          `UPDATE public.lenso_webhook_delivery SET generation=generation+1,updated_at=$3
           WHERE id=$1 AND generation=$2 AND state IN ('pending','retry') RETURNING *`,
          [id, generation, now],
        );
        return updated.rows[0] ? delivery(updated.rows[0]) : null;
      });
    },
    prune(cutoff, limit) {
      return transaction(async (client) => {
        const maximum = boundedLimit(limit);
        // A replay parent remains until all its descendants have themselves been pruned.
        const removed = await client.query(
          `WITH candidates AS (
             SELECT d.id FROM public.lenso_webhook_delivery d
             WHERE d.state IN ('succeeded','failed') AND d.updated_at<$1
               AND NOT EXISTS (SELECT 1 FROM public.lenso_webhook_delivery child WHERE child.replay_of=d.id)
             ORDER BY d.updated_at,d.id LIMIT $2 FOR UPDATE OF d SKIP LOCKED
           ) DELETE FROM public.lenso_webhook_delivery d USING candidates c
             WHERE d.id=c.id RETURNING d.id`,
          [cutoff, maximum],
        );
        // Zero-recipient events also need retention. Never remove a referenced event.
        await client.query(
          `WITH candidates AS (
             SELECT e.id FROM public.lenso_webhook_event e WHERE e.created_at<$1
               AND NOT EXISTS (SELECT 1 FROM public.lenso_webhook_delivery d WHERE d.event_id=e.id)
             ORDER BY e.created_at,e.id LIMIT $2 FOR UPDATE OF e SKIP LOCKED
           ) DELETE FROM public.lenso_webhook_event e USING candidates c WHERE e.id=c.id`,
          [cutoff, maximum],
        );
        return removed.rowCount ?? 0;
      });
    },
  };
}
