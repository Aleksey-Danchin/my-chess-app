export function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`;
}

export function formatProgress(
  downloaded: number,
  total: number | null | undefined,
): string {
  if (total && total > 0) {
    const pct = Math.min(100, Math.round((downloaded / total) * 100));
    return `${formatBytes(downloaded)} / ${formatBytes(total)} (${pct}%)`;
  }
  return formatBytes(downloaded);
}
