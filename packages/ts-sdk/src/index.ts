import { GeneratedLensoClient, type LensoClientOptions } from './generated/client.js';

export type {
  AdminFunctionRunDetail,
  AdminFunctionRunListResponse,
  AdminFunctionRunResponse,
  AdminOutboxEventDetail,
  AdminOutboxEventDetailResponse,
  AdminOutboxListResponse,
  AdminRuntimeFunctionRunItem,
  AdminRuntimeFunctionDeclarationMetadata,
  AdminRuntimeFunctionSummary,
  AdminRuntimeHeatmapCell,
  AdminRuntimeHeatmapResponse,
  AdminRuntimeOutboxItem,
  AdminRuntimeOutboxSummary,
  AdminRuntimeStoryDetail,
  AdminRuntimeStoryDetailResponse,
  AdminRuntimeStoryEdge,
  AdminRuntimeStoryListItem,
  AdminRuntimeStoryListResponse,
  AdminRuntimeStoryNode,
  AdminRuntimeSummaryItem,
  AdminRuntimeSummaryResponse,
  AdminRuntimeTechnicalOperation,
  AdminRuntimeTechnicalOperationListResponse,
  AdminRuntimeTimelineItem,
  CreateUserRequest,
  CreateUserResponse,
  CreateUserResponseEnvelope,
  ErrorBody,
  ErrorResponse,
  MeResponse,
  MeResponseEnvelope,
  PageInfo,
  ValidationErrorDetail,
} from './generated/types.js';
export { LensoApiError, type LensoClientOptions } from './generated/client.js';

export type LensoClient = {
  readonly identity: {
    createUser(
      input: Parameters<GeneratedLensoClient['createUser']>[0],
    ): ReturnType<GeneratedLensoClient['createUser']>;
  };
};

export function createClient(options: LensoClientOptions): LensoClient {
  const generated = new GeneratedLensoClient(options);

  return {
    identity: {
      createUser: (input) => generated.createUser(input),
    },
  };
}
