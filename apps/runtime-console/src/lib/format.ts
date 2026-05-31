import type { Actor } from "../data/mock-runtime";

const dateTime = new Intl.DateTimeFormat("en", {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
});

export function time(value?: string) {
  if (!value) {
    return "—";
  }

  return dateTime.format(new Date(value));
}

export function relativeAge(value: string) {
  const diff = Math.max(0, Date.now() - new Date(value).getTime());
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) {
    return "now";
  }
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  return `${Math.floor(minutes / 60)}h ago`;
}

export function duration(start?: string, end?: string) {
  if (!start) {
    return "—";
  }

  const finish = end ? new Date(end).getTime() : Date.now();
  const ms = Math.max(0, finish - new Date(start).getTime());
  if (ms < 1000) {
    return `${ms}ms`;
  }

  return `${(ms / 1000).toFixed(1)}s`;
}

export function actorLabel(actor: Actor) {
  switch (actor.kind) {
    case "anonymous": {
      return "anonymous";
    }
    case "user": {
      return `user:${actor.id}`;
    }
    case "service": {
      return `service:${actor.id}`;
    }
    case "system": {
      return "system";
    }
    default: {
      return "unknown";
    }
  }
}

export function prettyJson(value: unknown) {
  return JSON.stringify(value, null, 2);
}
