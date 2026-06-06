import type { RuntimeStory } from "../../app/console-host-api";

export function resolveSelectedRuntimeStory(
  visibleStories: RuntimeStory[],
  selectedCorrelationId: string | null,
  detailClosed: boolean
) {
  if (detailClosed) {
    return null;
  }

  return (
    visibleStories.find(
      (story) => story.correlationId === selectedCorrelationId
    ) ??
    visibleStories.find((story) => story.id === selectedCorrelationId) ??
    visibleStories[0] ??
    null
  );
}
