import type {
  Attributes,
  Condition,
  Predicate,
  Resource,
  Scope,
  Permission,
  Request,
} from "./types";

export function all<A extends string = string, R extends Resource = Resource, C = Attributes>(
  ...conditions: readonly Condition<A, R, C>[]
): Condition<A, R, C> {
  return { kind: "all", conditions };
}

export function any<A extends string = string, R extends Resource = Resource, C = Attributes>(
  ...conditions: readonly Condition<A, R, C>[]
): Condition<A, R, C> {
  return { kind: "any", conditions };
}

export function predicate<A extends string = string, R extends Resource = Resource, C = Attributes>(
  test: Predicate<A, R, C>,
): Condition<A, R, C> {
  return { kind: "predicate", test };
}

export function attribute(
  source: "principal" | "resource" | "context",
  key: string,
  operator: "equals" | "in",
  value: string | number | boolean | null | readonly (string | number | boolean | null)[],
): Condition {
  return { kind: "attribute", source, key, operator, value };
}

export function relation(name: string, target?: Resource): Condition {
  return { kind: "relation", relation: name, ...(target ? { target } : {}) };
}

export function sameScope(left: Scope, right: Scope): boolean {
  return left.type === right.type && left.id === right.id;
}

export function matchesPermission(
  permission: Permission,
  request: Request<string, Resource, unknown>,
): boolean {
  return (
    permission.action === request.action &&
    permission.resourceType === request.resource.type &&
    sameScope(permission.scope, request.resource.scope) &&
    (permission.resourceId === undefined || permission.resourceId === request.resource.id)
  );
}
