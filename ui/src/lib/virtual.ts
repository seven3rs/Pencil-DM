export interface Range {
  start: number;
  end: number;
}

/** Rows to render for a scroll position: the visible rows plus `overscan` on each side. */
export function visibleRange(
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  total: number,
  overscan = 6,
): Range {
  if (total <= 0 || rowHeight <= 0) return { start: 0, end: 0 };
  const first = Math.floor(Math.max(0, scrollTop) / rowHeight);
  const last = Math.ceil((Math.max(0, scrollTop) + Math.max(0, viewportHeight)) / rowHeight);
  return {
    start: Math.max(0, first - overscan),
    end: Math.min(total, last + overscan),
  };
}

/** The scrollTop that brings `index` fully into view with the least movement. */
export function scrollToReveal(
  index: number,
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
): number {
  const top = index * rowHeight;
  const bottom = top + rowHeight;
  if (top < scrollTop) return top;
  if (bottom > scrollTop + viewportHeight) return bottom - viewportHeight;
  return scrollTop;
}
