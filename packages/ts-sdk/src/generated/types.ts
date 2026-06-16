/* eslint-disable */
// Generated from contracts/openapi/app-api.v1.yaml. Do not edit by hand.

export type AdminAction = {
  capability: string;
  confirmation?: AdminActionConfirmation | null;
  danger_level?: AdminActionDangerLevel;
  input_schema?: AdminActionInputSchema | null;
  label: string;
  name: string;
};

export type AdminActionConfirmation = {
  message: string;
  required_phrase?: string | null;
};

export type AdminActionDangerLevel = string;

export type AdminActionInputField = {
  description?: string | null;
  field_type: FieldType;
  label: string;
  name: string;
  required?: boolean;
};

export type AdminActionInputSchema = {
  fields?: Array<AdminActionInputField>;
};

export type AdminActionInvocationDto = {
  correlation_id: string;
  request_id: string;
  story_node_id: string;
};

export type AdminActionInvocationItem = {
  action_name: string;
  capability?: string | null;
  correlation_id: string;
  duration_ms: number;
  error_code?: string | null;
  error_message?: string | null;
  id: string;
  input_summary?: string | null;
  label: string;
  module_name: string;
  occurred_at: string;
  request_id?: string | null;
  result_summary?: string | null;
  span_id?: string | null;
  success: boolean;
  trace_id?: string | null;
};

export type AdminActionInvocationListResponse = {
  data: Array<AdminActionInvocationItem>;
  page: PageInfo;
};

export type AdminActionInvokeRequest = {
  confirmation_phrase?: string | null;
  input?: unknown;
};

export type AdminActionInvokeResponse = {
  data: unknown;
  invocation: AdminActionInvocationDto;
};

export type AdminCapabilityIssueDto = {
  capability: string;
  message: string;
  subject: string;
  suggestion: string;
};

export type AdminCapabilitySummaryDto = {
  declared_count: number;
  missing_count: number;
  referenced_count: number;
  unused_count: number;
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

export type AdminDeclarativeComponent = {
  kind: string;
  metrics?: Array<AdminMetricBinding>;
} | {
  entity: string;
  kind: string;
};

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
  fallback_schema?: AdminSchema | null;
  pages?: Array<AdminDeclarativePage>;
};

export type AdminEmbeddedEntry = {
  allowed_origins?: Array<string>;
  kind: string;
  url: string;
};

export type AdminEmbeddedRuntime = string;

export type AdminEmbeddedSurface = {
  entry: AdminEmbeddedEntry;
  fallback_schema?: AdminSchema | null;
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
  runtime_declaration?: AdminRuntimeFunctionDeclarationMetadata | null;
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

export type AdminModuleActivationState = string;

export type AdminModuleCompatibilityDto = {
  consolePackageApi?: string | null;
  lenso?: AdminModuleLensoCompatibilityDto | null;
};

export type AdminModuleConsolePackagePlanPackageDto = {
  command?: string | null;
  exportName: string;
  key?: string | null;
  packageName: string;
  route?: string | null;
  status?: string | null;
};

export type AdminModuleConsolePackagePlanStateDto = {
  error?: string | null;
  exists: boolean;
  moduleEntryPresent: boolean;
  packageCount: number;
  packages: Array<AdminModuleConsolePackagePlanPackageDto>;
  planFile: string;
  readable: boolean;
  restartRequired?: boolean | null;
};

export type AdminModuleGovernanceDto = {
  activation_reasons: Array<string>;
  activation_state: AdminModuleActivationState;
  capability_issues: Array<AdminCapabilityIssueDto>;
  capability_summary: AdminCapabilitySummaryDto;
};

export type AdminModuleHostCompatibilityDto = {
  consolePackageApi: string;
  lensoVersion: string;
};

export type AdminModuleInstallStateDto = {
  consolePlan: AdminModuleConsolePackagePlanStateDto;
  moduleRegistered: boolean;
  remoteSource: AdminModuleRemoteSourceInstallStateDto;
};

export type AdminModuleLensoCompatibilityDto = {
  maxVersion?: string | null;
  minVersion?: string | null;
};

export type AdminModuleMetadataDto = {
  admin?: AdminSurface | null;
  capabilities: Array<string>;
  console: Array<ConsoleSurface>;
  error?: string | null;
  events?: EventSurface | null;
  governance: AdminModuleGovernanceDto;
  http_routes: Array<ModuleHttpRoute>;
  lifecycle?: LifecycleSurface | null;
  manifest_lints: Array<ModuleManifestLint>;
  module_name: string;
  runtime?: RuntimeSurface | null;
  source: ModuleSource;
  source_diagnostics?: AdminModuleSourceDiagnosticsDto | null;
  status: AdminModuleStatus;
  story_display: Array<StoryDisplayDescriptorDto>;
};

export type AdminModuleMetadataListResponse = {
  modules: Array<AdminModuleMetadataDto>;
  refresh_error?: string | null;
  refresh_history: Array<AdminModuleRefreshRecordDto>;
  refreshed_at?: string | null;
};

export type AdminModuleRefreshModuleResultDto = {
  duration_ms?: number | null;
  endpoint?: string | null;
  error?: string | null;
  module_name: string;
  source: ModuleSource;
  status: AdminModuleRefreshModuleStatusDto;
};

export type AdminModuleRefreshModuleStatusDto = string;

export type AdminModuleRefreshRecordDto = {
  completed_at: string;
  duration_ms: number;
  error?: string | null;
  id: string;
  module_count: number;
  module_results: Array<AdminModuleRefreshModuleResultDto>;
  started_at: string;
  status: AdminModuleRefreshStatusDto;
};

export type AdminModuleRefreshStatusDto = string;

export type AdminModuleRegistrySnapshotCatalogDto = {
  modules: number;
  registryFile: string;
  version: number;
};

export type AdminModuleRegistrySnapshotIssueDto = {
  fix: string;
  group: string;
  message: string;
};

export type AdminModuleRegistrySnapshotManifestStatus = string;

export type AdminModuleRegistrySnapshotModuleDto = {
  archiveReason?: string | null;
  archivedAt?: string | null;
  baseUrl?: string | null;
  capabilities: Array<string>;
  catalogVersion: string;
  compatibility?: AdminModuleCompatibilityDto | null;
  consolePackageHints: number;
  hostCompatibility: AdminModuleHostCompatibilityDto;
  installState: AdminModuleInstallStateDto;
  manifestName?: string | null;
  manifestReference: string;
  manifestStatus: AdminModuleRegistrySnapshotManifestStatus;
  manifestVersion?: string | null;
  name: string;
  source: ModuleSource;
  status: AdminModuleRegistrySnapshotModuleStatus;
  summary?: string | null;
};

export type AdminModuleRegistrySnapshotModuleStatus = string;

export type AdminModuleRegistrySnapshotResponse = {
  catalog: AdminModuleRegistrySnapshotCatalogDto;
  issues: Array<AdminModuleRegistrySnapshotIssueDto>;
  modules: Array<AdminModuleRegistrySnapshotModuleDto>;
  status: AdminModuleRegistrySnapshotStatus;
  version: number;
};

export type AdminModuleRegistrySnapshotStatus = string;

export type AdminModuleRemoteSourceInstallStateDto = {
  configured: boolean;
  desiredBaseUrl?: string | null;
  envFile: string;
  error?: string | null;
  restartPending: boolean;
  restartReason?: string | null;
  runningBaseUrl?: string | null;
};

export type AdminModuleSchema = {
  error?: string | null;
  module_name: string;
  schema: AdminSchema;
  source: ModuleSource;
  status: AdminModuleStatus;
};

export type AdminModuleSourceDiagnosticsDto = unknown;

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

export type AdminPermission = {
  entity: string;
  kind: string;
} | {
  action: string;
  kind: string;
};

export type AdminRemoteModuleDiagnosticsDto = {
  auth_configured: boolean;
  base_url: string;
  last_checked_at?: string | null;
  last_load_error?: string | null;
  load_duration_ms?: number | null;
  manifest_url: string;
  timeout_ms: number;
};

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

export type AdminRuntimeFunctionDeclarationMetadata = {
  input_schema?: string | null;
  module_name: string;
  module_source: ModuleSource;
  name: string;
  queue: string;
  retry_policy?: RuntimeRetryPolicyDeclaration | null;
  version: number;
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
  runtime_declaration?: AdminRuntimeFunctionDeclarationMetadata | null;
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

export type ConsoleArea = string;

export type ConsoleNavigation = {
  group?: ConsoleNavigationGroup | null;
  order?: number | null;
  workspace: ConsoleWorkspaceRef;
};

export type ConsoleNavigationGroup = {
  icon?: string | null;
  id: string;
  label: string;
  order?: number | null;
};

export type ConsolePackage = {
  export: string;
  name: string;
};

export type ConsoleSurface = {
  area: ConsoleArea;
  icon?: string | null;
  label: string;
  name: string;
  navigation?: ConsoleNavigation | null;
  package: ConsolePackage;
  required_capabilities?: Array<string>;
  route: string;
};

export type ConsoleWorkspaceRef = {
  icon?: string | null;
  id: string;
  label: string;
};

export type CreateDevSessionRequest = {
  user_id: string;
};

export type CreateDevSessionResponse = {
  expires_at: string;
  session_id: string;
  token: string;
  user_id: string;
};

export type CreateDevSessionResponseEnvelope = {
  data: CreateDevSessionResponse;
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

export type EventHandlerDeclaration = {
  event_name: string;
  name: string;
};

export type EventSurface = {
  handlers?: Array<EventHandlerDeclaration>;
};

export type FieldSchema = {
  field_type: FieldType;
  label: string;
  name: string;
  nullable?: boolean;
};

export type FieldType = {
  kind: string;
};

export type LifecycleActivationJobDeclaration = {
  function_name: string;
  input?: unknown;
  name: string;
  required?: boolean;
  run_policy?: LifecycleActivationRunPolicy;
};

export type LifecycleActivationRunPolicy = string;

export type LifecycleStartupCheckDeclaration = unknown;

export type LifecycleStartupCheckKind = {
  function_name: string;
  kind: string;
} | {
  capability: string;
  kind: string;
};

export type LifecycleSurface = {
  activation_jobs?: Array<LifecycleActivationJobDeclaration>;
  startup_checks?: Array<LifecycleStartupCheckDeclaration>;
};

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

export type ModuleManifestLint = {
  message: string;
  severity: ModuleManifestLintSeverity;
  subject: string;
  suggestion: string;
};

export type ModuleManifestLintSeverity = string;

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

export type RevokeSessionResponse = {
  revoked: boolean;
};

export type RevokeSessionResponseEnvelope = {
  data: RevokeSessionResponse;
};

export type RuntimeFunctionDeclaration = {
  input_schema?: string | null;
  name: string;
  queue: string;
  retry_policy?: RuntimeRetryPolicyDeclaration | null;
  version: number;
};

export type RuntimeRetryPolicyDeclaration = {
  initial_delay_ms: number;
  max_attempts: number;
};

export type RuntimeSurface = {
  functions?: Array<RuntimeFunctionDeclaration>;
};

export type StoryDisplayDescriptorDto = {
  display_name: string;
  source: StoryDisplaySourceDto;
  story_title?: string | null;
};

export type StoryDisplaySourceDto = {
  kind: string;
  name: string;
} | {
  kind: string;
  method: string;
  path: string;
};

export type ValidationErrorDetail = {
  field?: string | null;
  reason: string;
};

