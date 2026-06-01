import type { RuntimeStory } from "../../data/mock-runtime";

export type CommandItem = {
  id: string;
  title: string;
  subtitle: string;
  action: () => void;
  searchText: string;
};

export function buildStoryCommandItems({
  onOpenStory,
  stories,
  limit = 12,
}: {
  stories: RuntimeStory[];
  onOpenStory: (storyId: string) => void;
  limit?: number;
}): CommandItem[] {
  return stories.slice(0, limit).map((story) => ({
    action: () => onOpenStory(story.id),
    id: `story:${story.correlationId}`,
    searchText: storySearchText(story),
    subtitle: `${story.status} · ${story.correlationId}`,
    title: story.name,
  }));
}

function storySearchText(story: RuntimeStory) {
  return [
    story.id,
    story.name,
    story.service,
    story.source,
    story.status,
    story.correlationId,
    ...story.nodes.flatMap((node) => [
      node.id,
      node.name,
      node.service,
      node.kind,
      node.status,
    ]),
  ]
    .join(" ")
    .toLowerCase();
}
