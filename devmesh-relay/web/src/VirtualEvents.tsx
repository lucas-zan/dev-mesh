import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";

export function visibleRows(
  heights: number[],
  top: number,
  height: number,
  overscan = 4,
) {
  const offsets = [0];
  for (const size of heights) offsets.push(offsets.at(-1)! + size);
  let first = 0,
    last = 0;
  while (first < heights.length && offsets[first + 1] <= top) first++;
  last = first;
  while (last < heights.length && offsets[last] < top + height) last++;
  const start = Math.max(0, first - overscan),
    end = Math.min(heights.length, last + overscan);
  return {
    start,
    end,
    before: offsets[start],
    after: offsets.at(-1)! - offsets[end],
  };
}

export function VirtualEvents({
  ids,
  scrollContainer,
  renderRow,
}: {
  ids: string[];
  scrollContainer: RefObject<HTMLDivElement | null>;
  renderRow: (index: number) => ReactNode;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [sizes, setSizes] = useState<Map<string, number>>(() => new Map());
  const [viewport, setViewport] = useState({ top: 0, height: 600 });
  const range = visibleRows(
    ids.map((id) => sizes.get(id) ?? 128),
    viewport.top,
    viewport.height,
  );
  useEffect(() => {
    const container = scrollContainer.current;
    if (!container) return;
    const update = () => {
      if (!root.current) return;
      setViewport({
        top: Math.max(
          0,
          container.getBoundingClientRect().top -
            root.current.getBoundingClientRect().top,
        ),
        height: container.clientHeight,
      });
    };
    const observer = new ResizeObserver(update);
    observer.observe(container);
    container.addEventListener("scroll", update, { passive: true });
    update();
    return () => {
      observer.disconnect();
      container.removeEventListener("scroll", update);
    };
  }, [scrollContainer]);
  useEffect(() => {
    const observer = new ResizeObserver((entries) => {
      setSizes((previous) => {
        const next = new Map(previous);
        let changed = false;
        for (const entry of entries) {
          const id = (entry.target as HTMLElement).dataset.row!;
          const height = entry.target.getBoundingClientRect().height;
          if (height > 0 && next.get(id) !== height) {
            next.set(id, height);
            changed = true;
          }
        }
        return changed ? next : previous;
      });
    });
    root.current
      ?.querySelectorAll(":scope > [data-row]")
      .forEach((row) => observer.observe(row));
    return () => observer.disconnect();
  }, [ids, range.start, range.end]);
  return (
    <div ref={root}>
      <div aria-hidden="true" style={{ height: range.before }} />
      {ids.slice(range.start, range.end).map((id, offset) => (
        <div key={id} data-row={id} style={{ display: "flow-root" }}>
          {renderRow(range.start + offset)}
        </div>
      ))}
      <div aria-hidden="true" style={{ height: range.after }} />
    </div>
  );
}
