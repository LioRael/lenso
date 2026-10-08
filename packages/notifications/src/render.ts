import type { EmailMessage, NotificationTemplate } from "./contracts";
import { NotificationError } from "./errors";

export function hasControlCharacters(value: string): boolean {
  return Array.from(value).some(
    (character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
  );
}

export function identifier(value: unknown): asserts value is string {
  if (
    typeof value !== "string" ||
    !value.trim() ||
    value.length > 256 ||
    hasControlCharacters(value)
  ) {
    throw new NotificationError("invalid-input");
  }
}

export function email(value: unknown): asserts value is string {
  if (
    typeof value !== "string" ||
    value.length > 320 ||
    hasControlCharacters(value) ||
    !/^[^\s<>@]+@[^\s<>@]+\.[^\s<>@]+$/.test(value)
  ) {
    throw new NotificationError("invalid-input");
  }
}

export function canonical(value: unknown, depth = 0): string {
  if (depth > 32) throw new NotificationError("invalid-input");
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value === "number" && Number.isFinite(value)) return JSON.stringify(value);
  if (!value || typeof value !== "object") throw new NotificationError("invalid-input");
  if (Array.isArray(value)) {
    if (Reflect.ownKeys(value).length !== value.length + 1)
      throw new NotificationError("invalid-input");
    const items: string[] = [];
    for (let index = 0; index < value.length; index++) {
      const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
      if (!descriptor || !descriptor.enumerable || !("value" in descriptor)) {
        throw new NotificationError("invalid-input");
      }
      items.push(canonical(descriptor.value, depth + 1));
    }
    return `[${items.join(",")}]`;
  }
  if (![Object.prototype, null].includes(Object.getPrototypeOf(value))) {
    throw new NotificationError("invalid-input");
  }
  const keys = Reflect.ownKeys(value);
  if (keys.some((key) => typeof key !== "string")) throw new NotificationError("invalid-input");
  const entries = (keys as string[]).sort();
  return `{${entries
    .map((key) => {
      const descriptor = Object.getOwnPropertyDescriptor(value, key)!;
      if (typeof key !== "string" || !descriptor.enumerable || !("value" in descriptor)) {
        throw new NotificationError("invalid-input");
      }
      return `${JSON.stringify(key)}:${canonical(descriptor.value, depth + 1)}`;
    })
    .join(",")}}`;
}

export async function digest(value: string): Promise<string> {
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function escapeHtml(text: string): string {
  return text.replace(
    /[&<>"']/g,
    (character) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character]!,
  );
}

export async function renderTemplate(
  template: NotificationTemplate,
  variables: unknown,
  recipient: string,
): Promise<EmailMessage> {
  let result;
  try {
    result = await template.variables["~standard"].validate(variables);
  } catch {
    throw new NotificationError("invalid-input");
  }
  if (
    result.issues ||
    !result.value ||
    typeof result.value !== "object" ||
    Array.isArray(result.value)
  ) {
    throw new NotificationError("invalid-input");
  }
  canonical(result.value);
  const values = result.value as Record<string, unknown>;
  if (
    Object.values(values).some(
      (value) =>
        !["string", "boolean", "number"].includes(typeof value) ||
        (typeof value === "number" && !Number.isFinite(value)),
    )
  )
    throw new NotificationError("invalid-input");
  const interpolate = (source: string) => {
    const rendered = source.replace(/\{\{([a-zA-Z][a-zA-Z0-9_]*)\}\}/g, (_, key: string) => {
      if (!Object.hasOwn(values, key)) throw new NotificationError("invalid-input");
      return String(values[key]);
    });
    if (source.replace(/\{\{([a-zA-Z][a-zA-Z0-9_]*)\}\}/g, "").includes("{{")) {
      throw new NotificationError("invalid-template");
    }
    return rendered;
  };
  const subject = interpolate(template.subject);
  const text = interpolate(template.text);
  if (
    !subject.trim() ||
    subject.length > 998 ||
    hasControlCharacters(subject) ||
    text.length > 100_000
  ) {
    throw new NotificationError("invalid-input");
  }
  email(template.from);
  email(recipient);
  return {
    from: template.from,
    to: recipient,
    subject,
    text,
    html: `<div>${escapeHtml(text).replace(/\r?\n/g, "<br>")}</div>`,
  };
}
