import type {
  FunctionRun,
  RuntimeEvent,
  RuntimeRecord,
  RuntimeStory,
} from "../../data/mock-runtime";

export type RuntimeSearchResult =
  | {
      kind: "event";
      id: string;
      title: string;
      subtitle: string;
      correlationId: string;
      record: RuntimeRecord;
    }
  | {
      kind: "function";
      id: string;
      title: string;
      subtitle: string;
      correlationId: string;
      record: RuntimeRecord;
    }
  | {
      kind: "story";
      id: string;
      title: string;
      subtitle: string;
      correlationId: string;
      storyId: string;
      nodeId?: string;
    }
  | {
      kind: "correlation";
      id: string;
      title: string;
      subtitle: string;
      correlationId: string;
    };

export function buildRuntimeSearchResults({
  events,
  functions,
  query,
  stories,
  limit = 8,
}: {
  query: string;
  stories: RuntimeStory[];
  events: RuntimeEvent[];
  functions: FunctionRun[];
  limit?: number;
}): RuntimeSearchResult[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return [];
  }

  const storyResults: RuntimeSearchResult[] = stories.flatMap((story) => {
    const matchesStory = [
      story.id,
      story.name,
      story.service,
      story.source,
      story.status,
      story.correlationId,
    ].some((value) => value.toLowerCase().includes(normalized));

    const matchingNodes = story.nodes.filter((node) =>
      [node.id, node.name, node.service, node.kind, node.status].some((value) =>
        value.toLowerCase().includes(normalized)
      )
    );

    return [
      ...(matchesStory
        ? [
            {
              kind: "story" as const,
              id: story.id,
              title: story.name,
              subtitle: `${story.status} · ${story.correlationId}`,
              correlationId: story.correlationId,
              storyId: story.id,
            },
          ]
        : []),
      ...matchingNodes.map<RuntimeSearchResult>((node) => ({
        kind: "story",
        id: node.id,
        title: node.name,
        subtitle: `${story.correlationId} · ${node.service}`,
        correlationId: story.correlationId,
        storyId: story.id,
        nodeId: node.id,
      })),
    ];
  });

  const eventResults: RuntimeSearchResult[] = events
    .filter((event) =>
      [
        event.id,
        event.eventName,
        event.status,
        event.correlationId,
        event.lastError ?? "",
      ].some((value) => value.toLowerCase().includes(normalized))
    )
    .map((event) => ({
      kind: "event",
      id: event.id,
      title: event.eventName,
      subtitle: `${event.status} · ${event.correlationId}`,
      correlationId: event.correlationId,
      record: { kind: "event", item: event },
    }));

  const functionResults: RuntimeSearchResult[] = functions
    .filter((run) =>
      [
        run.id,
        run.functionName,
        run.status,
        run.correlationId,
        run.lastError ?? "",
      ].some((value) => value.toLowerCase().includes(normalized))
    )
    .map((run) => ({
      kind: "function",
      id: run.id,
      title: run.functionName,
      subtitle: `${run.status} · ${run.correlationId}`,
      correlationId: run.correlationId,
      record: { kind: "function", item: run },
    }));

  const correlations = Array.from(
    new Set([
      ...stories.map((story) => story.correlationId),
      ...events.map((event) => event.correlationId),
      ...functions.map((run) => run.correlationId),
    ])
  )
    .filter((id) => id.toLowerCase().includes(normalized))
    .map<RuntimeSearchResult>((id) => ({
      kind: "correlation",
      id,
      title: id,
      subtitle: "Open correlation in Stories",
      correlationId: id,
    }));

  return [
    ...storyResults,
    ...correlations,
    ...eventResults,
    ...functionResults,
  ].slice(0, limit);
}
