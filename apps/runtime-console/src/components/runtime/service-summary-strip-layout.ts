export const serviceSummaryHeaderHeight = 28;
export const serviceSummaryDefaultHeight = 144;

export function getServiceSummaryPanelLayout({
  expanded,
  headerHeight = serviceSummaryHeaderHeight,
  height = serviceSummaryDefaultHeight,
}: {
  expanded: boolean;
  headerHeight?: number | undefined;
  height?: number | undefined;
}) {
  const expandedHeight = Math.max(headerHeight, height);
  return {
    contentHeight: expanded ? Math.max(0, expandedHeight - headerHeight) : 0,
    panelHeight: expanded ? expandedHeight : headerHeight,
  };
}
