export type CompletedTask = { status: string; created: number; completedAt?: number | null };

export function recentCompleted<T extends CompletedTask>(tasks: T[]): T[] {
  return tasks.filter(task => task.status === "Completed")
    .sort((a, b) => (b.completedAt ?? b.created) - (a.completedAt ?? a.created))
    .slice(0, 2);
}

export function fitSidebars(width: number, requestedLeft: number, requestedRight: number, showRight = true) {
  const gap = showRight ? 24 : 12;
  const maxLeft = Math.max(120, Math.min(320, width - 360 - (showRight ? 220 : 0) - gap));
  const left = Math.min(maxLeft, Math.max(Math.min(160, maxLeft), Number.isFinite(requestedLeft) ? requestedLeft : 208));
  const right = showRight ? Math.max(220, Math.min(420, width - left - 360 - gap, Number.isFinite(requestedRight) ? requestedRight : 280)) : 0;
  return { left, right };
}

export function unifiedQueue<T extends { created: number; dismissed?: boolean }>(tasks: T[]): T[] {
  return tasks.filter(task => !task.dismissed).sort((a, b) => b.created - a.created);
}
