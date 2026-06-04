import type { TechnicalOperation } from "../../data/mock-runtime";

export type TechnicalOperationView = TechnicalOperation & {
  relativeStartMs: number;
  safeAttributes: Record<string, unknown>;
  sourceLabel: string;
  summary: string | undefined;
};

export type TechnicalOperationGroup = {
  id: string;
  label: string;
  category: TechnicalOperation["category"] | "story";
  operations: TechnicalOperationView[];
};

export function buildTechnicalOperationGroups(input: {
  executionOperations: TechnicalOperation[];
  selectedNodeId: string;
  storyOperations: TechnicalOperation[];
  storyTimestamp: string;
}): TechnicalOperationGroup[] {
  const executionOperations = input.executionOperations.filter(
    (operation) => operation.relatedNodeId === input.selectedNodeId
  );
  const storyLevelOperations = input.storyOperations.filter(
    (operation) => !operation.relatedNodeId
  );

  return [
    ...groupByCategory(executionOperations, input.storyTimestamp),
    ...storyGroup(storyLevelOperations, input.storyTimestamp),
  ];
}

export function technicalOperationsStateLabel(input: {
  isLoading: boolean;
  isError: boolean;
  error: unknown;
}) {
  if (input.isLoading) {
    return "Loading technical operations...";
  }
  if (input.isError) {
    return "Technical operations could not be loaded.";
  }
  return "No technical operations recorded for this execution.";
}

function groupByCategory(
  operations: TechnicalOperation[],
  storyTimestamp: string
): TechnicalOperationGroup[] {
  const groups = new Map<
    TechnicalOperation["category"],
    TechnicalOperation[]
  >();
  for (const operation of operations) {
    const current = groups.get(operation.category) ?? [];
    current.push(operation);
    groups.set(operation.category, current);
  }

  return [...groups.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([category, items]) => ({
      category,
      id: category,
      label: category,
      operations: operationViews(items, storyTimestamp),
    }));
}

function storyGroup(
  operations: TechnicalOperation[],
  storyTimestamp: string
): TechnicalOperationGroup[] {
  if (operations.length === 0) {
    return [];
  }

  return [
    {
      category: "story",
      id: "story-level",
      label: "Story-level operations",
      operations: operationViews(operations, storyTimestamp),
    },
  ];
}

function operationViews(
  operations: TechnicalOperation[],
  storyTimestamp: string
): TechnicalOperationView[] {
  const storyStart = Date.parse(storyTimestamp);
  return operations
    .map((operation) => ({
      ...operation,
      relativeStartMs:
        Number.isFinite(storyStart) &&
        Number.isFinite(Date.parse(operation.startedAt))
          ? Math.max(0, Date.parse(operation.startedAt) - storyStart)
          : 0,
      safeAttributes: safeAttributes(operation.attributes),
      sourceLabel: technicalOperationSourceLabel(operation),
      summary: technicalOperationSummary(operation),
    }))
    .sort(
      (left, right) =>
        left.relativeStartMs - right.relativeStartMs ||
        left.name.localeCompare(right.name)
    );
}

export function technicalOperationSourceLabel(operation: TechnicalOperation) {
  return operation.source === "remote_proxy" ? "remote proxy" : "otel";
}

export function technicalOperationSummary(operation: TechnicalOperation) {
  if (operation.source !== "remote_proxy") {
    return;
  }

  const moduleName = stringAttribute(operation.attributes.module_name);
  const method = stringAttribute(operation.attributes.method);
  const declaredPath = stringAttribute(operation.attributes.declared_path);
  const remotePath = stringAttribute(operation.attributes.remote_path);
  const remoteStatus = numberAttribute(operation.attributes.remote_status);
  const requestId = stringAttribute(operation.attributes.request_id);
  const parts = [
    moduleName,
    [method, declaredPath].filter(Boolean).join(" "),
    remotePath ? `remote ${remotePath}` : undefined,
    typeof remoteStatus === "number" ? `status ${remoteStatus}` : undefined,
    requestId ? `request ${requestId}` : undefined,
  ].filter(Boolean);

  return parts.length > 0 ? parts.join(" / ") : undefined;
}

function stringAttribute(value: unknown) {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function numberAttribute(value: unknown) {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function safeAttributes(attributes: Record<string, unknown>) {
  return Object.fromEntries(
    Object.entries(attributes).filter(([key, value]) => {
      if (!isSafeAttributeKey(key)) {
        return false;
      }
      return (
        value === null ||
        typeof value === "string" ||
        typeof value === "number" ||
        typeof value === "boolean"
      );
    })
  );
}

function isSafeAttributeKey(key: string) {
  const lower = key.toLowerCase();
  return ![
    "authorization",
    "cookie",
    "password",
    "secret",
    "token",
    "api_key",
    "email",
    "statement",
    "query",
    "body",
    "payload",
  ].some((unsafePart) => lower.includes(unsafePart));
}
