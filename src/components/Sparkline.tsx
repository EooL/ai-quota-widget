import type { HistoryPoint } from "../lib/types";

export default function Sparkline({ points, color, id }: { points: HistoryPoint[]; color: string; id: string }) {
  if (points.length < 2) return <div className="history-empty" data-testid={`sparkline-${id}`}>24h history: collecting…</div>;
  const recent = points.filter((point) => point.t >= Date.now() - 24 * 3600_000);
  const values = (recent.length > 1 ? recent : points).map((point) =>
    Math.max(0, Math.min(100, (1 - point.remaining / Math.max(point.limit, 1)) * 100)),
  );
  const max = 100;
  const coords = values.map((value, index) => `${(index / (values.length - 1)) * 100},${28 - (value / max) * 24}`).join(" ");
  const area = `0,28 ${coords} 100,28`;
  const gradientId = `gradient-${id}`;
  return <div className="sparkline-wrap" data-testid={`sparkline-${id}`}>
    <div className="sparkline-label"><span>24h history</span><span>now</span></div>
    <svg className="h-7 w-full" viewBox="0 0 100 28" preserveAspectRatio="none" aria-label="Quota over the last 24 hours">
    <defs><linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor={color} stopOpacity=".35" /><stop offset="1" stopColor={color} stopOpacity="0" /></linearGradient></defs>
    <polygon points={area} fill={`url(#${gradientId})`} />
    <polyline points={coords} fill="none" stroke={color} strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
    </svg>
  </div>;
}
