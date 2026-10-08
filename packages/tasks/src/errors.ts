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
  constructor(readonly code: TaskQueueErrorCode) {
    super(messages[code]);
    this.name = "TaskQueueError";
  }
}
