type InspectorEscapeEvent = {
  altKey: boolean;
  ctrlKey: boolean;
  defaultPrevented: boolean;
  key: string;
  metaKey: boolean;
  target: EventTarget | null;
};

export function shouldCloseInspectorOnEscape(event: InspectorEscapeEvent) {
  if (event.defaultPrevented || event.key !== "Escape") {
    return false;
  }

  if (event.metaKey || event.ctrlKey || event.altKey) {
    return false;
  }

  const target = event.target as
    | (EventTarget & {
        closest?: (selector: string) => Element | null;
        isContentEditable?: boolean;
        tagName?: string;
      })
    | null;
  const tagName = target?.tagName;
  return !(
    tagName === "INPUT" ||
    tagName === "TEXTAREA" ||
    tagName === "SELECT" ||
    target?.isContentEditable ||
    target?.closest?.('[role="dialog"]')
  );
}
