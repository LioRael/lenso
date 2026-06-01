import { GeneratedLensoClient, type LensoClientOptions } from './generated/client.js';

export type {
  AdminFunctionRun,
  AdminFunctionRunDetail,
  AdminFunctionRunListResponse,
  AdminFunctionRunResponse,
  AdminOutboxEvent,
  AdminOutboxEventDetail,
  AdminOutboxEventDetailResponse,
  AdminOutboxListResponse,
  AdminRuntimeFunctionSummary,
  AdminRuntimeHeatmapCell,
  AdminRuntimeHeatmapResponse,
  AdminRuntimeOutboxSummary,
  AdminRuntimeStoryDetail,
  AdminRuntimeStoryDetailResponse,
  AdminRuntimeStoryEdge,
  AdminRuntimeStoryListItem,
  AdminRuntimeStoryListResponse,
  AdminRuntimeStoryNode,
  AdminRuntimeSummaryItem,
  AdminRuntimeSummaryResponse,
  AdminRuntimeTimelineItem,
  AdminRuntimeTimelineResponse,
  CreateUserRequest,
  CreateUserResponse,
  ErrorBody,
  ErrorResponse,
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
