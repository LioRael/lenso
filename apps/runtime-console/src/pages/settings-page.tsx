import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { History, RotateCcw } from "lucide-react";
import { useMemo, useState } from "react";

import type {
  ConfigAuditDto,
  ConfigAuditListResponse,
  ConfigDescriptorDto,
  ConfigDescriptorListResponse,
  ConfigValueDto,
  ConfigValueListResponse,
  ConfigWriteResponse,
} from "../../../../packages/ts-sdk/src/generated/types";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Drawer } from "../components/ui/drawer";
import { time } from "../lib/format";
import {
  httpClient,
  isApiMode,
  runtimeConsoleDataSource,
} from "../lib/http-client";

type ValueType =
  | { kind: "bool" }
  | { kind: "int"; min?: number; max?: number }
  | { kind: "float"; min?: number; max?: number }
  | { kind: "string" }
  | { kind: "enum"; values: string[] }
  | { kind: "json" };

type ConfigRow = {
  descriptor: ConfigDescriptorDto;
  valueType: ValueType;
  value: unknown;
  source: ConfigValueDto["source"];
};

const configQueryKeys = {
  descriptors: ["config", "descriptors"] as const,
  values: ["config", "values"] as const,
  audit: (service: string, key: string) =>
    ["config", "audit", service, key] as const,
};

function configPath(service: string, key: string) {
  return `admin/config/${encodeURIComponent(service)}/${encodeURIComponent(key)}`;
}

function parseValueType(raw: unknown): ValueType {
  if (raw && typeof raw === "object" && "kind" in raw) {
    const candidate = raw as { kind: unknown };
    switch (candidate.kind) {
      case "bool":
      case "string":
      case "json": {
        return { kind: candidate.kind };
      }
      case "int":
      case "float": {
        const numeric = raw as { min?: unknown; max?: unknown };
        return {
          kind: candidate.kind,
          ...(typeof numeric.min === "number" ? { min: numeric.min } : {}),
          ...(typeof numeric.max === "number" ? { max: numeric.max } : {}),
        };
      }
      case "enum": {
        const enumeral = raw as { values?: unknown };
        return {
          kind: "enum",
          values: Array.isArray(enumeral.values)
            ? enumeral.values.filter(
                (item): item is string => typeof item === "string"
              )
            : [],
        };
      }
      default: {
        return { kind: "json" };
      }
    }
  }
  return { kind: "json" };
}

function useConfigDescriptors() {
  return useQuery({
    enabled: isApiMode(),
    queryKey: configQueryKeys.descriptors,
    queryFn: () =>
      httpClient
        .get("admin/config/descriptors")
        .json<ConfigDescriptorListResponse>(),
  });
}

function useConfigValues() {
  return useQuery({
    enabled: isApiMode(),
    queryKey: configQueryKeys.values,
    queryFn: () =>
      httpClient.get("admin/config/values").json<ConfigValueListResponse>(),
  });
}

export function SettingsPage() {
  if (!isApiMode()) {
    return <DeferredSettings />;
  }
  return <ConfigSettings />;
}

function ConfigSettings() {
  const descriptorsQuery = useConfigDescriptors();
  const valuesQuery = useConfigValues();
  const [auditTarget, setAuditTarget] = useState<ConfigRow | null>(null);

  const rows = useMemo<ConfigRow[]>(() => {
    const descriptors = descriptorsQuery.data?.data ?? [];
    const values = valuesQuery.data?.data ?? [];
    const valueByKey = new Map(values.map((item) => [item.key, item]));
    return descriptors
      .map((descriptor) => {
        const match = valueByKey.get(descriptor.key);
        return {
          descriptor,
          valueType: parseValueType(descriptor.value_type),
          value: match ? match.value : descriptor.default,
          source: match ? match.source : "default",
        };
      })
      .sort(
        (a, b) =>
          a.descriptor.service.localeCompare(b.descriptor.service) ||
          a.descriptor.key.localeCompare(b.descriptor.key)
      );
  }, [descriptorsQuery.data, valuesQuery.data]);

  const grouped = useMemo(() => {
    const map = new Map<string, ConfigRow[]>();
    for (const row of rows) {
      const bucket = map.get(row.descriptor.service) ?? [];
      bucket.push(row);
      map.set(row.descriptor.service, bucket);
    }
    return [...map.entries()];
  }, [rows]);

  const isLoading = descriptorsQuery.isLoading || valuesQuery.isLoading;
  const error = descriptorsQuery.error ?? valuesQuery.error;

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div className="flex items-center gap-2">
          <h1 className="font-mono text-[13px] font-semibold">Configuration</h1>
          <span className="ml-auto font-mono text-[10px] text-(--muted)">
            {rows.length} keys / {runtimeConsoleDataSource()}
          </span>
        </div>
      </header>

      <div className="min-h-0 overflow-auto">
        {isLoading ? (
          <LoadingRows />
        ) : error ? (
          <MessageRow message={errorMessage(error)} tone="error" />
        ) : rows.length === 0 ? (
          <MessageRow message="no configuration descriptors registered" />
        ) : (
          grouped.map(([service, serviceRows]) => (
            <div key={service}>
              <div className="sticky top-0 z-10 border-b border-(--border-subtle) bg-(--sidebar) px-3 py-1.5 font-mono text-[10px] font-semibold uppercase tracking-[0.12em] text-(--accent)">
                {service === "*" ? "shared" : service}
              </div>
              {serviceRows.map((row) => (
                <ConfigRowEditor
                  key={`${row.descriptor.service}:${row.descriptor.key}`}
                  onOpenAudit={() => setAuditTarget(row)}
                  row={row}
                />
              ))}
            </div>
          ))
        )}
      </div>

      <AuditDrawer onClose={() => setAuditTarget(null)} target={auditTarget} />
    </section>
  );
}

function ConfigRowEditor({
  onOpenAudit,
  row,
}: {
  onOpenAudit: () => void;
  row: ConfigRow;
}) {
  const queryClient = useQueryClient();
  const { descriptor, valueType } = row;
  const [draft, setDraft] = useState<string>(() =>
    toInputString(row.value, valueType)
  );
  const [localError, setLocalError] = useState<string | null>(null);

  const invalidateValues = () =>
    queryClient.invalidateQueries({ queryKey: configQueryKeys.values });

  const saveMutation = useMutation({
    mutationFn: (value: unknown) =>
      httpClient
        .put(configPath(descriptor.service, descriptor.key), {
          json: { value },
        })
        .json<ConfigWriteResponse>(),
    onSuccess: async () => {
      setLocalError(null);
      await invalidateValues();
    },
    onError: (mutationError: unknown) =>
      setLocalError(errorMessage(mutationError)),
  });

  const resetMutation = useMutation({
    mutationFn: () =>
      httpClient
        .delete(configPath(descriptor.service, descriptor.key))
        .json<ConfigWriteResponse>(),
    onSuccess: async () => {
      setLocalError(null);
      await invalidateValues();
    },
    onError: (mutationError: unknown) =>
      setLocalError(errorMessage(mutationError)),
  });

  const submit = () => {
    const parsed = parseDraft(draft, valueType);
    if (!parsed.ok) {
      setLocalError(parsed.error);
      return;
    }
    saveMutation.mutate(parsed.value);
  };

  const pending = saveMutation.isPending || resetMutation.isPending;
  const disabled = !descriptor.editable;

  return (
    <div className="grid gap-1.5 border-b border-(--border-subtle) px-3 py-2 font-mono text-[11px]">
      <div className="flex items-center gap-2">
        <span className="truncate font-semibold text-(--foreground)">
          {descriptor.key}
        </span>
        <SourceBadge source={row.source} />
        {disabled ? <Tag tone="muted">read-only</Tag> : null}
        {descriptor.restart_only ? (
          <Tag tone="warn">applies on restart</Tag>
        ) : null}
        <button
          aria-label="View audit history"
          className="ml-auto inline-flex h-6 items-center gap-1 border border-(--border-subtle) px-1.5 text-[10px] text-(--muted) hover:text-(--foreground)"
          onClick={onOpenAudit}
          type="button"
        >
          <History size={11} />
          audit
        </button>
      </div>

      {descriptor.description ? (
        <span className="text-[10px] text-(--muted)">
          {descriptor.description}
        </span>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <ValueControl
          disabled={disabled}
          label={descriptor.key}
          onChange={setDraft}
          onSubmit={submit}
          value={draft}
          valueType={valueType}
        />
        <Button
          disabled={disabled || pending}
          onClick={submit}
          variant="default"
        >
          Save
        </Button>
        <Button
          disabled={disabled || pending}
          onClick={() => resetMutation.mutate()}
          variant="ghost"
        >
          <RotateCcw size={12} />
          Reset
        </Button>
      </div>

      {localError ? (
        <span className="text-[10px] text-(--error)">{localError}</span>
      ) : null}
    </div>
  );
}

function ValueControl({
  disabled,
  label,
  onChange,
  onSubmit,
  value,
  valueType,
}: {
  disabled: boolean;
  label: string;
  onChange: (next: string) => void;
  onSubmit: () => void;
  value: string;
  valueType: ValueType;
}) {
  const inputClass =
    "h-6 min-w-[160px] border border-(--border-subtle) bg-(--elevated) px-2 text-[11px] text-(--foreground) outline-hidden focus-visible:border-(--accent) disabled:opacity-45";

  if (valueType.kind === "bool") {
    return (
      <label className="inline-flex h-6 items-center gap-1.5 text-[11px] text-(--secondary)">
        <input
          aria-label={label}
          checked={value === "true"}
          className="size-3.5 accent-(--accent) disabled:opacity-45"
          disabled={disabled}
          onChange={(event) =>
            onChange(event.target.checked ? "true" : "false")
          }
          type="checkbox"
        />
        {value === "true" ? "true" : "false"}
      </label>
    );
  }

  if (valueType.kind === "enum") {
    return (
      <select
        aria-label={label}
        className={inputClass}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        value={value}
      >
        {valueType.values.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    );
  }

  if (valueType.kind === "int" || valueType.kind === "float") {
    return (
      <input
        aria-label={label}
        className={inputClass}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            onSubmit();
          }
        }}
        step={valueType.kind === "float" ? "any" : "1"}
        type="number"
        value={value}
        {...(valueType.min === undefined ? {} : { min: valueType.min })}
        {...(valueType.max === undefined ? {} : { max: valueType.max })}
      />
    );
  }

  if (valueType.kind === "json") {
    return (
      <textarea
        aria-label={label}
        className={`${inputClass} h-16 w-full resize-y py-1 font-mono`}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        spellCheck={false}
        value={value}
      />
    );
  }

  return (
    <input
      aria-label={label}
      className={inputClass}
      disabled={disabled}
      onChange={(event) => onChange(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          onSubmit();
        }
      }}
      type="text"
      value={value}
    />
  );
}

function AuditDrawer({
  onClose,
  target,
}: {
  onClose: () => void;
  target: ConfigRow | null;
}) {
  const service = target?.descriptor.service ?? "";
  const key = target?.descriptor.key ?? "";
  const auditQuery = useQuery({
    enabled: Boolean(target),
    queryKey: configQueryKeys.audit(service, key),
    queryFn: () =>
      httpClient
        .get(`${configPath(service, key)}/audit`)
        .json<ConfigAuditListResponse>(),
  });
  const entries = auditQuery.data?.data ?? [];

  return (
    <Drawer
      onOpenChange={(open) => (open ? null : onClose())}
      open={Boolean(target)}
    >
      <Drawer.Content>
        <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2 font-mono">
          <Drawer.Title className="text-[13px] font-semibold text-(--foreground)">
            Audit
          </Drawer.Title>
          {target ? (
            <div className="mt-1 truncate text-[10px] text-(--muted)">
              {target.descriptor.service === "*"
                ? "shared"
                : target.descriptor.service}{" "}
              / {target.descriptor.key}
            </div>
          ) : null}
        </header>
        <div className="p-3 font-mono text-[11px]">
          {auditQuery.isLoading ? (
            <MessageRow message="loading audit history" />
          ) : auditQuery.isError ? (
            <MessageRow message={errorMessage(auditQuery.error)} tone="error" />
          ) : entries.length === 0 ? (
            <MessageRow message="no audit entries" />
          ) : (
            <ol className="grid gap-2">
              {entries.map((entry, index) => (
                <AuditEntry
                  entry={entry}
                  key={`${entry.changed_at}:${index}`}
                />
              ))}
            </ol>
          )}
        </div>
      </Drawer.Content>
    </Drawer>
  );
}

function AuditEntry({ entry }: { entry: ConfigAuditDto }) {
  return (
    <li className="border border-(--border-subtle) bg-(--elevated) px-2.5 py-2">
      <div className="flex items-center gap-2 text-[10px] text-(--muted)">
        <span>{entry.actor ?? "system"}</span>
        <span className="ml-auto">{time(entry.changed_at)}</span>
      </div>
      <div className="mt-1 grid gap-1">
        <div className="flex gap-2">
          <span className="w-8 shrink-0 text-(--muted)">old</span>
          <span className="min-w-0 break-words text-(--secondary)">
            {previewValue(entry.old_value)}
          </span>
        </div>
        <div className="flex gap-2">
          <span className="w-8 shrink-0 text-(--muted)">new</span>
          <span className="min-w-0 break-words text-(--foreground)">
            {previewValue(entry.new_value)}
          </span>
        </div>
      </div>
    </li>
  );
}

function SourceBadge({ source }: { source: string }) {
  return (
    <Badge className="px-1.5 py-0.5 text-[9px] uppercase tracking-[0.08em]">
      {source}
    </Badge>
  );
}

function Tag({ children, tone }: { children: string; tone: "muted" | "warn" }) {
  return (
    <span
      className={`border px-1.5 py-0.5 text-[9px] uppercase tracking-[0.08em] ${
        tone === "warn"
          ? "border-[color-mix(in_srgb,var(--warning)_35%,transparent)] text-(--warning)"
          : "border-(--border-subtle) text-(--muted)"
      }`}
    >
      {children}
    </span>
  );
}

function toInputString(value: unknown, valueType: ValueType): string {
  if (valueType.kind === "bool") {
    return value === true ? "true" : "false";
  }
  if (valueType.kind === "json") {
    return JSON.stringify(value, null, 2);
  }
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  return String(value);
}

type ParseResult = { ok: true; value: unknown } | { ok: false; error: string };

function parseDraft(draft: string, valueType: ValueType): ParseResult {
  switch (valueType.kind) {
    case "bool": {
      return { ok: true, value: draft === "true" };
    }
    case "int": {
      const parsed = Number(draft);
      if (!Number.isInteger(parsed)) {
        return { ok: false, error: "value must be an integer" };
      }
      return { ok: true, value: parsed };
    }
    case "float": {
      const parsed = Number(draft);
      if (!Number.isFinite(parsed) || draft.trim() === "") {
        return { ok: false, error: "value must be a number" };
      }
      return { ok: true, value: parsed };
    }
    case "json": {
      try {
        return { ok: true, value: JSON.parse(draft) as unknown };
      } catch {
        return { ok: false, error: "invalid JSON" };
      }
    }
    default: {
      return { ok: true, value: draft };
    }
  }
}

function previewValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "—";
  }
  if (typeof value === "string") {
    return value;
  }
  return JSON.stringify(value);
}

function LoadingRows() {
  return (
    <>
      <div className="h-12 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-12 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-12 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
    </>
  );
}

function MessageRow({
  message,
  tone = "muted",
}: {
  message: string;
  tone?: "error" | "muted";
}) {
  return (
    <div
      className={`border-b border-(--border-subtle) px-3 py-3 font-mono text-[11px] ${
        tone === "error" ? "text-(--error)" : "text-(--muted)"
      }`}
    >
      {message}
    </div>
  );
}

function DeferredSettings() {
  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div className="flex items-center gap-2">
          <h1 className="font-mono text-[13px] font-semibold">Configuration</h1>
          <span className="ml-auto font-mono text-[10px] text-(--muted)">
            deferred
          </span>
        </div>
      </header>
      <div className="p-3 font-mono">
        <div className="border-y border-(--border-subtle)">
          <div className="grid grid-cols-[96px_minmax(0,1fr)] border-b border-(--border-subtle) text-[11px]">
            <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">
              status
            </div>
            <div className="px-3 py-1.5 text-(--secondary)">
              requires API mode
            </div>
          </div>
          <div className="grid grid-cols-[96px_minmax(0,1fr)] text-[11px]">
            <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">
              reason
            </div>
            <div className="px-3 py-1.5 text-(--secondary)">
              configuration is unavailable in mock mode
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error
    ? error.message
    : "Configuration request failed";
}
