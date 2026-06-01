/* eslint-disable */
// Generated from contracts/openapi/app-api.v1.yaml. Do not edit by hand.

export type AdminFunctionRunDetail = {
  actor: unknown;
  attempts: number;
  available_at: string;
  completed_at?: string | null;
  correlation_id: string;
  created_at: string;
  function_name: string;
  id: string;
  input_json: unknown;
  last_error?: string | null;
  locked_by?: string | null;
  max_attempts: number;
  started_at?: string | null;
  status: string;
};

export type AdminFunctionRunListResponse = {
  data: Array<AdminRuntimeFunctionRunItem>;
  page: PageInfo;
};

export type AdminFunctionRunResponse = {
  data: AdminFunctionRunDetail;
};

export type AdminOutboxEventDetail = {
  actor: unknown;
  aggregate_id: string;
  aggregate_type: string;
  attempts: number;
  available_at: string;
  causation_id?: string | null;
  correlation_id: string;
  created_at: string;
  event_name: string;
  event_version: number;
  headers: unknown;
  id: string;
  last_error?: string | null;
  locked_by?: string | null;
  max_attempts: number;
  occurred_at: string;
  payload: unknown;
  published_at?: string | null;
  source_module: string;
  status: string;
  trace: unknown;
};

export type AdminOutboxEventDetailResponse = {
  data: AdminOutboxEventDetail;
};

export type AdminOutboxListResponse = {
  data: Array<AdminRuntimeOutboxItem>;
  page: PageInfo;
};

export type AdminRuntimeFunctionRunItem = {
  attempts: number;
  available_at: string;
  completed_at?: string | null;
  correlation_id: string;
  created_at: string;
  function_name: string;
  id: string;
  last_error?: string | null;
  locked_by?: string | null;
  max_attempts: number;
  started_at?: string | null;
  status: string;
};

export type AdminRuntimeFunctionSummary = {
  completed: number;
  dead: number;
  failed: number;
  oldest_failed_age_seconds?: number | null;
  oldest_pending_age_seconds?: number | null;
  pending: number;
  running: number;
};

export type AdminRuntimeHeatmapCell = {
  avg_duration_ms?: number | null;
  bucket_end: string;
  bucket_start: string;
  dead_count: number;
  error_count: number;
  max_duration_ms?: number | null;
  node_type: string;
  retry_count: number;
  service: string;
  total_count: number;
};

export type AdminRuntimeHeatmapResponse = {
  bucket_seconds: number;
  data: Array<AdminRuntimeHeatmapCell>;
  order: string;
  page: PageInfo;
};

export type AdminRuntimeOutboxItem = {
  attempts: number;
  available_at: string;
  correlation_id: string;
  created_at: string;
  event_name: string;
  id: string;
  last_error?: string | null;
  locked_by?: string | null;
  max_attempts: number;
  published_at?: string | null;
  status: string;
};

export type AdminRuntimeOutboxSummary = {
  dead: number;
  failed: number;
  oldest_failed_age_seconds?: number | null;
  oldest_pending_age_seconds?: number | null;
  pending: number;
  processing: number;
  published: number;
};

export type AdminRuntimeStoryDetail = {
  edges: Array<AdminRuntimeStoryEdge>;
  nodes: Array<AdminRuntimeStoryNode>;
  summary: AdminRuntimeStoryListItem;
  timeline_items: Array<AdminRuntimeTimelineItem>;
};

export type AdminRuntimeStoryDetailResponse = {
  data: AdminRuntimeStoryDetail;
};

export type AdminRuntimeStoryEdge = {
  id: string;
  label?: string | null;
  source: string;
  target: string;
  type: string;
};

export type AdminRuntimeStoryListItem = {
  correlation_id: string;
  created_at: string;
  duration: number;
  error_count: number;
  node_count: number;
  pattern: Array<string>;
  root_error?: string | null;
  services: Array<string>;
  status: string;
  title: string;
  updated_at: string;
};

export type AdminRuntimeStoryListResponse = {
  data: Array<AdminRuntimeStoryListItem>;
  order: string;
  page: PageInfo;
};

export type AdminRuntimeStoryNode = {
  duration_ms: number;
  error?: string | null;
  id: string;
  metadata: unknown;
  name: string;
  service: string;
  status: string;
  timestamp: string;
  type: string;
};

export type AdminRuntimeSummaryItem = {
  attempts: number;
  correlation_id?: string | null;
  created_at: string;
  id: string;
  last_error?: string | null;
  max_attempts: number;
  name: string;
  status: string;
  type: string;
};

export type AdminRuntimeSummaryResponse = {
  functions: AdminRuntimeFunctionSummary;
  outbox: AdminRuntimeOutboxSummary;
  recent_activity: Array<AdminRuntimeSummaryItem>;
  recent_failures: Array<AdminRuntimeSummaryItem>;
  status: string;
};

export type AdminRuntimeTimelineItem = {
  attempts: number;
  completed_at?: string | null;
  correlation_id: string;
  created_at: string;
  id: string;
  last_error?: string | null;
  max_attempts: number;
  name: string;
  related_node_id?: string | null;
  started_at?: string | null;
  status: string;
  type: string;
};

export type AdminRuntimeTimelineResponse = {
  data: Array<AdminRuntimeTimelineItem>;
  order: string;
  page: PageInfo;
};

export type CreateUserRequest = {
  display_name?: string | null;
  email: string;
};

export type CreateUserResponse = {
  created_at: string;
  display_name?: string | null;
  email: string;
  id: string;
};

export type CreateUserResponseEnvelope = {
  data: CreateUserResponse;
};

export type ErrorBody = {
  code: string;
  correlation_id?: string | null;
  details: Array<ValidationErrorDetail>;
  message: string;
  request_id?: string | null;
};

export type ErrorResponse = {
  error: ErrorBody;
};

export type MeResponse = {
  scopes: Array<string>;
  user_id: string;
};

export type MeResponseEnvelope = {
  data: MeResponse;
};

export type PageInfo = {
  limit: number;
  next_created_before?: string | null;
};

export type ValidationErrorDetail = {
  field?: string | null;
  reason: string;
};

