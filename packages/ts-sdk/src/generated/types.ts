/* eslint-disable */
// Generated from contracts/openapi/app-api.v1.yaml. Do not edit by hand.

export type AdminAction = {
  capability: string;
  label: string;
  name: string;
};

export type AdminDataDetailResponse = {
  data: unknown;
};

export type AdminDataListResponse = {
  data: Array<unknown>;
  page: AdminDataPageInfo;
};

export type AdminDataPageInfo = {
  limit: number;
  next_cursor?: string | null;
};

export type AdminDeclarativeComponent = unknown;

export type AdminDeclarativePage = {
  label: string;
  name: string;
  sections?: Array<AdminDeclarativeSection>;
};

export type AdminDeclarativeSection = {
  component: AdminDeclarativeComponent;
  label: string;
  name: string;
};

export type AdminDeclarativeSurface = {
  actions?: Array<AdminAction>;
  fallback_schema?: unknown;
  pages?: Array<AdminDeclarativePage>;
};

export type AdminEmbeddedEntry = unknown;

export type AdminEmbeddedRuntime = string;

export type AdminEmbeddedSurface = {
  entry: AdminEmbeddedEntry;
  fallback_schema?: unknown;
  permissions?: Array<AdminPermission>;
  runtime: AdminEmbeddedRuntime;
  sandbox: AdminSandboxPolicy;
};

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

export type AdminMetricBinding = {
  label: string;
  value_path: string;
};

export type AdminModuleMetadataDto = {
  admin?: unknown;
  error?: string | null;
  http_routes: Array<ModuleHttpRoute>;
  module_name: string;
  source: ModuleSource;
  status: AdminModuleStatus;
};

export type AdminModuleMetadataListResponse = {
  modules: Array<AdminModuleMetadataDto>;
};

export type AdminModuleSchema = {
  error?: string | null;
  module_name: string;
  schema: AdminSchema;
  source: ModuleSource;
  status: AdminModuleStatus;
};

export type AdminModuleStatus = string;

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

export type AdminPermission = unknown;

export type AdminRemoteProxyCallItem = {
  capability?: string | null;
  correlation_id: string;
  declared_path: string;
  duration_ms: number;
  error_code?: string | null;
  error_details: unknown;
  id: string;
  method: string;
  module_name: string;
  occurred_at: string;
  path_params: unknown;
  remote_path: string;
  remote_status?: number | null;
  request_id: string;
  retryable: boolean;
  span_id?: string | null;
  success: boolean;
  trace_id?: string | null;
};

export type AdminRemoteProxyCallListResponse = {
  data: Array<AdminRemoteProxyCallItem>;
  page: PageInfo;
};

export type AdminRuntimeExecutionLog = {
  attributes: unknown;
  body: string;
  correlation_id: string;
  execution_name: string;
  id: string;
  node_id: string;
  node_type: string;
  occurred_at: string;
  redacted_fields: Array<string>;
  service_name: string;
  severity: string;
  span_id?: string | null;
  story_id: string;
  trace_id?: string | null;
};

export type AdminRuntimeExecutionLogListResponse = {
  data: Array<AdminRuntimeExecutionLog>;
  order: string;
  page: PageInfo;
};

export type AdminRuntimeExecutionPayload = {
  input: unknown;
  metadata: unknown;
  node_id: string;
  node_type: string;
  output?: unknown;
  redacted_fields: Array<string>;
};

export type AdminRuntimeExecutionPayloadResponse = {
  data: AdminRuntimeExecutionPayload;
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
  display_name: string;
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

export type AdminRuntimeTechnicalOperation = {
  attributes: unknown;
  category: string;
  correlation_id: string;
  duration_ms: number;
  ended_at: string;
  id: string;
  name: string;
  related_node_id?: string | null;
  source: string;
  started_at: string;
  status: string;
  story_id: string;
};

export type AdminRuntimeTechnicalOperationListResponse = {
  data: Array<AdminRuntimeTechnicalOperation>;
  order: string;
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

export type AdminSandboxPolicy = {
  allow_forms?: boolean;
  allow_popups?: boolean;
  allow_same_origin?: boolean;
  allow_scripts?: boolean;
};

export type AdminSchema = {
  entities: Array<EntitySchema>;
};

export type AdminSchemaListResponse = {
  modules: Array<AdminModuleSchema>;
};

export type AdminSchemaRefreshResponse = {
  modules: Array<AdminModuleSchema>;
};

export type AdminSurface = unknown;

export type ConfigAuditDto = {
  actor?: string | null;
  changed_at: string;
  key: string;
  new_value: unknown;
  old_value?: unknown;
  service: string;
};

export type ConfigAuditListResponse = {
  data: Array<ConfigAuditDto>;
};

export type ConfigDescriptorDto = {
  default: unknown;
  description: string;
  editable: boolean;
  key: string;
  restart_only: boolean;
  service: string;
  value_type: unknown;
};

export type ConfigDescriptorListResponse = {
  data: Array<ConfigDescriptorDto>;
};

export type ConfigValueDto = {
  key: string;
  source: string;
  value: unknown;
};

export type ConfigValueListResponse = {
  data: Array<ConfigValueDto>;
};

export type ConfigWriteRequest = {
  value: unknown;
};

export type ConfigWriteResponse = {
  applies_on_restart: boolean;
  key: string;
  service: string;
  updated_at: string;
  updated_by?: string | null;
  value: unknown;
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

export type EntitySchema = {
  fields: Array<FieldSchema>;
  label: string;
  name: string;
  read_capability: string;
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

export type FieldSchema = {
  field_type: FieldType;
  label: string;
  name: string;
  nullable?: boolean;
};

export type FieldType = unknown;

export type MeResponse = {
  scopes: Array<string>;
  user_id: string;
};

export type MeResponseEnvelope = {
  data: MeResponse;
};

export type ModuleHttpMethod = string;

export type ModuleHttpRoute = {
  capability?: string | null;
  display_name?: string | null;
  method: ModuleHttpMethod;
  path: string;
  story_title?: string | null;
};

export type ModuleSource = string;

export type PageInfo = {
  limit: number;
  next_created_before?: string | null;
};

export type RemoteHttpProxyResponse = {
  capability: string;
  data: unknown;
  declared_path: string;
  method: ModuleHttpMethod;
  module_name: string;
  path_params: Record<string, unknown>;
  remote_path: string;
  status: RemoteHttpProxyStatus;
};

export type RemoteHttpProxyStatus = string;

export type ValidationErrorDetail = {
  field?: string | null;
  reason: string;
};

