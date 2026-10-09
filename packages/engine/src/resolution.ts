/** Custom conditions are opt-in launch policy, not inferred from the checkout. */
export function runtimeConditions(args: readonly string[] = process.execArgv): string[] {
  const conditions: string[] = [];
  for (let index = 0; index < args.length; index++) {
    const arg = args[index]!;
    const value =
      arg === "--conditions" || arg === "-u"
        ? args[++index]
        : arg.startsWith("--conditions=")
          ? arg.slice("--conditions=".length)
          : undefined;
    if (value) conditions.push(value);
  }
  return [...new Set(conditions)];
}

export function conditionArgs(args: readonly string[] = process.execArgv): string[] {
  return runtimeConditions(args).map((condition) => `--conditions=${condition}`);
}
