import test from "node:test";
import assert from "node:assert/strict";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { IssueDirectory } from "./IssueDirectory.tsx";

const projects = [
  {
    project: { id: "p1", name: "team/alpha" },
    issues: [{ id: "i1", title: "Fix header" }],
  },
  {
    project: { id: "p2", name: "team/beta" },
    issues: [{ id: "i2", title: "Update README" }],
  },
  { project: { id: "p3", name: "team/new" }, issues: [] },
];
const render = (search = "") =>
  renderToStaticMarkup(
    createElement(IssueDirectory, {
      projects,
      search,
      selectedId: "i2",
      onSelect: () => {},
    }),
  );

test("shows separate project headings and keeps a selected Issue in its group", () => {
  const html = render();
  assert.match(
    html,
    /team\/alpha[\s\S]*Fix header[\s\S]*team\/beta[\s\S]*Update README/,
  );
  assert.doesNotMatch(html, /team\/new/);
  assert.match(html, /aria-current="true"[^>]*>[\s\S]*Update README/);
});

test("search finds an Issue belonging to another project", () => {
  const html = render(" readme ");
  assert.match(html, /team\/beta[\s\S]*Update README/);
  assert.doesNotMatch(html, /team\/alpha|Fix header|team\/new/);
});

test("closed or completed Issues are hidden until history is requested", () => {
  const projects = [
    {
      project: { id: "p1", name: "team/alpha" },
      issues: [
        { id: "i1", title: "Open issue", source_state: "open" },
        { id: "i2", title: "Closed issue", source_state: "closed" },
        { id: "i3", title: "Completed work", canonical_state: "done" },
      ],
    },
  ];
  const render = (showClosed: boolean) =>
    renderToStaticMarkup(
      createElement(IssueDirectory, {
        projects,
        search: "",
        showClosed,
        onSelect: () => {},
      }),
    );
  assert.match(render(false), /Open issue/);
  assert.doesNotMatch(render(false), /Closed issue|Completed work/);
  assert.match(render(true), /Closed issue[\s\S]*Completed work/);
});
