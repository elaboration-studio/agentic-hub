import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { UsageSourceBucket } from "@/types";
import { TOOL_LABELS } from "@/shared";
import type { ToolId } from "@/types";
import { CHART_COLORS, chartMargin } from "./chartTheme";

interface SourceToolChartProps {
  data: UsageSourceBucket[];
}

function sourceLabel(sourceTool: string): string {
  if (sourceTool === "agentic-hub") return "Palette";
  return TOOL_LABELS[sourceTool as ToolId] ?? sourceTool;
}

export function SourceToolChart({ data }: SourceToolChartProps) {
  if (data.length === 0) {
    return <p className="text-sm text-muted-foreground">No usage by tool yet.</p>;
  }

  const rows = data.map((row) => ({
    ...row,
    label: sourceLabel(row.sourceTool),
  }));

  return (
    <div className="h-[220px]">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={rows} layout="vertical" margin={{ ...chartMargin, left: 8 }}>
          <CartesianGrid stroke={CHART_COLORS.grid} horizontal={false} />
          <XAxis type="number" allowDecimals={false} hide />
          <YAxis
            type="category"
            dataKey="label"
            width={72}
            tick={{ fill: CHART_COLORS.axis, fontSize: 11 }}
            tickLine={false}
            axisLine={false}
          />
          <Tooltip
            contentStyle={{
              background: CHART_COLORS.tooltipBg,
              border: `1px solid ${CHART_COLORS.tooltipBorder}`,
              borderRadius: 8,
              fontSize: 12,
            }}
            formatter={(value) => [value, "Uses"]}
          />
          <Bar dataKey="executionCount" fill={CHART_COLORS.primary} radius={[0, 4, 4, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
