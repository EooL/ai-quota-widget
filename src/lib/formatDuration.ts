export function formatDuration(ms: number, compact = false): string {
  if (!Number.isFinite(ms)) return compact ? "—" : "--h --m --s";
  const sign = ms < 0 ? "-" : "";
  let remaining = Math.abs(Math.floor(ms / 1000));
  const days = Math.floor(remaining / 86400);
  remaining %= 86400;
  const hours = Math.floor(remaining / 3600);
  remaining %= 3600;
  const minutes = Math.floor(remaining / 60);
  const seconds = remaining % 60;

  if (compact) {
    if (days > 0) return `${sign}${days}d ${hours}h`;
    if (hours > 0) return `${sign}${hours}h ${minutes}m`;
    return `${sign}${minutes}m ${seconds}s`;
  }

  const pad = (n: number) => n.toString().padStart(2, "0");
  if (days > 0) {
    return `${sign}${days}d ${pad(hours)}h ${pad(minutes)}m`;
  }
  return `${sign}${pad(hours)}h ${pad(minutes)}m ${pad(seconds)}s`;
}
