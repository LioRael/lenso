export type TaskQueueErrorCode =
  | "invalid-task"
  | "invalid-input"
  | "invalid-result"
  | "invalid-options"
  | "deduplication-conflict"
  | "job-expired"
  | "provider-unavailable"
  | "closed";

const messages: Record<TaskQueueErrorCode, string> = {
  "invalid-task": "Task definition is invalid or is not registered in this queue",
  "invalid-input": "Task input must match its schema and fit the supported JSON limits",
  "invalid-result": "Task result must fit the supported JSON limits",
  "invalid-options": "Task queue options are invalid",
  "deduplication-conflict": "Deduplication key refers to a different task or input",
  "job-expired": "Deduplication key refers to a job outside its retention window",
  "provider-unavailable": "Task queue operation failed",
  closed: "Task queue is closed",
};

export class TaskQueueError extends Error {
  readonly code: TaskQueueErrorCode;

  constructor(code: TaskQueueErrorCode, options?: ErrorOptions) {
    const safeCode =
      typeof code === "string" && Object.hasOwn(messages, code) ? code : "provider-unavailable";
    super(messages[safeCode], options);
    this.code = safeCode;
    this.name = "TaskQueueError";
  }
}

export function taskErrorDiagnostic(error: unknown) {
  if (!(error instanceof TaskQueueError)) return undefined;
  const safe = new TaskQueueError(error.code);
  return { code: safe.code, phase: "invoke", message: safe.message } as const;
}
