export const CHART_COLORS = {
  primary: "#4f46e5",
  axis: "#9aa0ad",
  grid: "#272a33",
  tooltipBg: "#1d2029",
  tooltipBorder: "#272a33",
} as const;

export const KIND_CHART_COLORS: Record<string, string> = {
  skill: "#4f46e5",
  command: "#f59e0b",
  agent: "#a855f7",
};

export function kindChartColor(kind: string): string {
  return KIND_CHART_COLORS[kind] ?? CHART_COLORS.primary;
}

export const chartMargin = { top: 8, right: 8, left: 0, bottom: 0 } as const;
