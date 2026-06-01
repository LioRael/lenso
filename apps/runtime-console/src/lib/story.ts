import type {
  ExecutionNode,
  RuntimeStatus,
  RuntimeStory,
} from "../data/mock-runtime";

export type RuntimeNodeType =
  | "request"
  | "function"
  | "event"
  | "worker"
  | "external";

export type RuntimeNode = {
  id: string;
  type: RuntimeNodeType;
  typeLabel: string;
  name: string;
  status: RuntimeStatus;
  service: string;
  duration: number;
  timestamp: number;
  error?: string;
  parentId?: string;
  node: ExecutionNode;
};

export type RuntimeStorySummary = {
  id: string;
  title: string;
  correlationId: string;
  status: RuntimeStatus;
  duration: number;
  nodeCount: number;
  errorCount: number;
  services: string[];
  pattern: RuntimeNodeType[];
  patternLabel: string;
  rootError?: string;
  nodes: RuntimeNode[];
};

export function buildRuntimeStory(story: RuntimeStory): RuntimeStorySummary {
  const nodes = story.nodes.flatMap((node) => {
    const type = runtimeNodeType(node);

    if (!type) {
      return [];
    }

    const error = nodeError(node);

    return [
      {
        ...(error ? { error } : {}),
        ...(node.parentId ? { parentId: node.parentId } : {}),
        duration: node.durationMs,
        id: node.id,
        name: nodeName(node, type),
        service: node.service,
        node,
        status: node.status,
        timestamp: node.startMs,
        type,
        typeLabel: runtimeNodeTypeLabel(type),
      },
    ];
  });
  const services = Array.from(new Set(story.nodes.map((node) => node.service)));
  const pattern = collapsePattern(nodes.map((node) => node.type));
  const rootError = findRootError(nodes);

  return {
    ...(rootError ? { rootError } : {}),
    correlationId: story.correlationId,
    duration: story.durationMs,
    errorCount: story.nodes.filter(isErrorStatus).length,
    id: story.correlationId,
    nodeCount: nodes.length,
    nodes,
    pattern,
    patternLabel: pattern.map(runtimeNodeTypeLabel).join(" -> "),
    services,
    status: story.status,
    title: storyTitle(story, nodes),
  };
}

export function runtimeNodeType(node: ExecutionNode): RuntimeNodeType | null {
  if (node.kind === "http") {
    return "request";
  }

  if (node.kind === "command" || node.kind === "function") {
    return "function";
  }

  if (node.kind === "event") {
    return "event";
  }

  if (node.kind === "handler" || node.kind === "runtime") {
    return "worker";
  }

  if (node.kind === "external") {
    return "external";
  }

  return null;
}

export function runtimeNodeTypeLabel(type: RuntimeNodeType) {
  switch (type) {
    case "request": {
      return "Request";
    }
    case "function": {
      return "Function";
    }
    case "event": {
      return "Event";
    }
    case "worker": {
      return "Worker";
    }
    case "external": {
      return "External";
    }
    default: {
      const exhaustive: never = type;
      return exhaustive;
    }
  }
}

export function runtimeStatusIntent(status: RuntimeStatus) {
  if (status === "dead") {
    return "dead";
  }

  if (status === "failed") {
    return "failed";
  }

  if (status === "pending" || status === "processing") {
    return "retrying";
  }

  if (status === "running") {
    return "running";
  }

  return "success";
}

function storyTitle(story: RuntimeStory, nodes: RuntimeNode[]) {
  const root = nodes[0]?.node ?? story.nodes[0];

  if (root?.kind === "http" && root.name.includes("/identity/users")) {
    return "User Registration";
  }

  if (story.name.includes("object_uploaded")) {
    return "File Upload";
  }

  if (story.name.includes("cleanup_expired_sessions")) {
    return "Session Cleanup";
  }

  return humanizeRuntimeName(story.name);
}

function nodeName(node: ExecutionNode, type: RuntimeNodeType) {
  if (type === "external" && isErrorStatus(node)) {
    const error = nodeError(node);

    if (error?.toLowerCase().includes("smtp")) {
      return "smtp.provider";
    }
  }

  return humanizeRuntimeName(node.name);
}

function nodeError(node: ExecutionNode) {
  if (!isErrorStatus(node)) {
    return undefined;
  }

  const finalLog = node.logs.at(-1);

  if (finalLog?.toLowerCase().includes("smtp")) {
    return "smtp timeout";
  }

  if (finalLog) {
    return finalLog;
  }

  return `${node.status} runtime work`;
}

function findRootError(nodes: RuntimeNode[]) {
  for (let index = nodes.length - 1; index >= 0; index -= 1) {
    const node = nodes[index];
    if (node?.error) {
      return node.error;
    }
  }

  return undefined;
}

function collapsePattern(types: RuntimeNodeType[]) {
  return types.filter(
    (type, index) => index === 0 || types[index - 1] !== type
  );
}

function isErrorStatus(nodeOrStatus: ExecutionNode | RuntimeStatus) {
  const status =
    typeof nodeOrStatus === "string" ? nodeOrStatus : nodeOrStatus.status;
  return status === "failed" || status === "dead";
}

function humanizeRuntimeName(value: string) {
  return value.replace(/\.v\d+$/u, "");
}
