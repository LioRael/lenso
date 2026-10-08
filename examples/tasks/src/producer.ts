import { call } from "@lenso/cli";
import { resolve } from "node:path";
import { reportFailure } from "./config";

async function main() {
  const [command, id, ...extra] = process.argv.slice(2);
  if (
    extra.length ||
    !command ||
    (command === "enqueue" ? id !== undefined : !id) ||
    !["enqueue", "get", "cancel", "retry", "report"].includes(command)
  ) {
    console.error(
      "Usage: producer.ts enqueue < input.json | get|cancel|retry <jobId> | report <reportId>",
    );
    process.exitCode = 2;
    return;
  }
  const method = command === "enqueue" ? "submit" : command === "get" ? "query" : command;
  const input =
    command === "enqueue"
      ? JSON.parse(await Bun.stdin.text())
      : command === "report"
        ? { reportId: id }
        : { jobId: id };
  const result = await call(resolve(import.meta.dir, ".."), "tasks", method, input);
  console.log(command === "enqueue" ? (result as { jobId: string }).jobId : JSON.stringify(result));
}

if (import.meta.main) await main().catch(() => reportFailure("Producer"));
