import test from "node:test";
import assert from "node:assert/strict";
import { createElement, createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { VirtualEvents, visibleRows } from "./VirtualEvents.tsx";

test("virtual range accounts for viewport, overscan and measured row heights", () => {
  assert.deepEqual(visibleRows([], 0, 600, 2), {
    start: 0,
    end: 0,
    before: 0,
    after: 0,
  });
  const middle = visibleRows(Array(100).fill(100), 500, 300, 2);
  assert.deepEqual(middle, { start: 3, end: 10, before: 300, after: 9000 });
  const heights = [500, ...Array(9).fill(100)];
  assert.deepEqual(visibleRows(heights, 450, 100, 0), {
    start: 0,
    end: 2,
    before: 0,
    after: 800,
  });
});
test("a thousand messages mount a bounded viewport while retaining total height", () => {
  const html = renderToStaticMarkup(
    createElement(VirtualEvents, {
      ids: Array.from({ length: 1000 }, (_, i) => `e${i}`),
      scrollContainer: createRef<HTMLDivElement>(),
      renderRow: (index: number) =>
        createElement("p", { "data-message": index }, `message ${index}`),
    }),
  );
  assert.ok((html.match(/data-message=/g) ?? []).length < 30);
  assert.ok(html.includes("message 0"));
  assert.ok(!html.includes("message 999"));
  assert.ok(html.includes("aria-hidden"));
});
