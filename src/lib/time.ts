/**
 * Compact relative timestamps.
 *
 * Rows and headers show "2h ago" rather than a locale date: an absolute date is both wider
 * (it clipped whole rows in the review screenshots) and useless at a glance. The absolute value
 * stays available for tooltips, so nothing is lost.
 */

export function relativeTime(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return '';
  const minutes = Math.max(0, Math.round((Date.now() - then) / 60_000));
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.round(days / 30);
  if (months < 12) return `${months}mo ago`;
  return `${Math.round(months / 12)}y ago`;
}

export function absoluteTime(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return '';
  return new Date(then).toISOString().slice(0, 16).replace('T', ' ');
}
