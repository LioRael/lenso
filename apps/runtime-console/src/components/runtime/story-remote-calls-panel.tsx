import { useState } from "react";

import type { RuntimeStory } from "../../data/mock-runtime";
import {
  type RuntimeRemoteProxyCall,
  useRemoteProxyCalls,
} from "../../hooks/use-runtime-queries";
import { time } from "../../lib/format";
import { formatRuntimeDuration } from "../../lib/runtime-style";
import { JsonViewer } from "./json-viewer";

export function StoryRemoteCallsPanel({ story }: { story: RuntimeStory }) {
  const [selectedCallId, setSelectedCallId] = useState<string | null>(null);
  const remoteCallsQuery = useRemoteProxyCalls({
    correlationId: story.correlationId,
    limit: 8,
  });
  const calls = remoteCallsQuery.data?.pages.flatMap((page) => page.data) ?? [];
  const selectedCall = calls.find((call) => call.id === selectedCallId) ?? null;

  if (remoteCallsQuery.isLoading) {
    return <RemoteCallsMessage label="Loading remote calls..." />;
  }
  if (remoteCallsQuery.isError) {
    return (
      <RemoteCallsMessage
        label={`Remote calls unavailable. ${errorMessage(remoteCallsQuery.error)}`}
        tone="error"
      />
    );
  }

  return (
    <section className="min-h-0 border-b border-(--border-subtle) bg-(--background)">
      <div className="flex items-center gap-2 bg-(--sidebar) px-3 py-1.5 font-mono text-[11px] text-(--muted)">
        <span>Remote Calls</span>
        <span className="rounded-xs border border-(--border-subtle) bg-(--background) px-1.5 py-0.5 text-[10px] text-(--muted)">
          {calls.length}
        </span>
        <span className="ml-auto truncate text-[10px] text-(--muted-deep)">
          {story.correlationId}
        </span>
      </div>
      {calls.length === 0 ? (
        <RemoteCallsMessage label="No remote module calls recorded for this story" />
      ) : (
        <>
          <div className="grid h-7 grid-cols-[74px_136px_minmax(180px,1fr)_62px_72px_80px] items-center gap-2 border-t border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 font-mono text-[9px] uppercase tracking-[0.08em] text-(--muted)">
            <span>result</span>
            <span>module</span>
            <span>route</span>
            <span>status</span>
            <span>duration</span>
            <span>occurred</span>
          </div>
          {calls.map((call) => (
            <button
              className="grid min-h-9 w-full grid-cols-[74px_136px_minmax(180px,1fr)_62px_72px_80px] items-center gap-2 border-t border-(--border-subtle) px-3 text-left font-mono text-[10px] hover:bg-(--elevated)"
              key={call.id}
              onClick={() =>
                setSelectedCallId((current) =>
                  current === call.id ? null : call.id
                )
              }
              type="button"
            >
              <span
                className={call.success ? "text-[#22c55e]" : "text-[#ef4444]"}
              >
                {call.success ? "success" : "failed"}
              </span>
              <span className="truncate text-(--foreground)">
                {call.module_name}
              </span>
              <span className="truncate text-(--secondary)">
                {call.method} {call.declared_path}
              </span>
              <span className="text-(--muted)">{formatRemoteStatus(call)}</span>
              <span className="text-(--muted)">
                {formatRuntimeDuration(call.duration_ms)}
              </span>
              <span className="text-right text-(--muted)">
                {time(call.occurred_at)}
              </span>
            </button>
          ))}
          {selectedCall ? <StoryRemoteCallDetail call={selectedCall} /> : null}
        </>
      )}
    </section>
  );
}

function StoryRemoteCallDetail({ call }: { call: RuntimeRemoteProxyCall }) {
  return (
    <div className="border-t border-(--border-subtle) bg-(--surface)">
      <KeyValueRows
        rows={[
          ["request", call.request_id],
          ["trace", call.trace_id ?? "-"],
          ["span", call.span_id ?? "-"],
          ["remote path", call.remote_path],
          ["capability", call.capability ?? "-"],
          ["error code", call.error_code ?? "-"],
        ]}
      />
      <JsonViewer title="path params" value={call.path_params} />
      <JsonViewer title="error details" value={call.error_details} />
    </div>
  );
}

function KeyValueRows({ rows }: { rows: Array<[string, string]> }) {
  return (
    <div className="border-b border-(--border-subtle) font-mono text-[11px]">
      {rows.map(([key, value]) => (
        <div
          className="grid grid-cols-[104px_minmax(0,1fr)] border-b border-(--border-subtle) last:border-b-0"
          key={key}
        >
          <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">{key}</div>
          <div className="min-w-0 break-words px-3 py-1.5 text-(--secondary)">
            {value}
          </div>
        </div>
      ))}
    </div>
  );
}

function RemoteCallsMessage({
  label,
  tone = "muted",
}: {
  label: string;
  tone?: "error" | "muted";
}) {
  return (
    <section className="border-b border-(--border-subtle) bg-(--background) px-3 py-2 font-mono text-[11px]">
      <span className={tone === "error" ? "text-[#ef4444]" : "text-(--muted)"}>
        {label}
      </span>
    </section>
  );
}

function formatRemoteStatus(call: RuntimeRemoteProxyCall) {
  return call.remote_status === null || call.remote_status === undefined
    ? "-"
    : String(call.remote_status);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Runtime request failed";
}
