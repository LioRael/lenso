export function readConfig() {
  const connectionString = process.env.DATABASE_URL;
  if (!connectionString) throw new Error("DATABASE_URL is required.");
  return { connectionString, queueName: process.env.TASK_QUEUE_NAME ?? "reports" };
}

export function reportFailure(operation: string): void {
  console.error(`${operation} failed. Check configuration, migrations and database availability.`);
  process.exitCode = 1;
}
