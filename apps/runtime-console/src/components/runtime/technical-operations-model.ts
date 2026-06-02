import type { TechnicalOperation } from "../../data/mock-runtime";

export type TechnicalOperationView = TechnicalOperation & {
  relativeStartMs: number;
  safeAttributes: Record<string, unknown>;
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
    }))
    .sort(
      (left, right) =>
        left.relativeStartMs - right.relativeStartMs ||
        left.name.localeCompare(right.name)
    );
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
