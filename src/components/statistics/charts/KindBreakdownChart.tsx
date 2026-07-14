import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { UsageKindBucket } from "@/types";
import { KIND_LABEL } from "@/shared";
import type { CapabilityKind } from "@/types";
import { CHART_COLORS, chartMargin, kindChartColor } from "./chartTheme";

interface KindBreakdownChartProps {
  data: UsageKindBucket[];
}

function kindLabel(kind: string): string {
  if (kind in KIND_LABEL) {
    return KIND_LABEL[kind as CapabilityKind];
  }
  return kind;
}

export function KindBreakdownChart({ data }: KindBreakdownChartProps) {
  if (data.length === 0) {
    return <p className="text-sm text-muted-foreground">No usage by kind yet.</p>;
  }

  const rows = data.map((row) => ({
    ...row,
    label: kindLabel(row.kind),
    fill: kindChartColor(row.kind),
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
          <Bar dataKey="executionCount" radius={[0, 4, 4, 0]}>
            {rows.map((row) => (
              <Cell key={row.kind} fill={row.fill} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
