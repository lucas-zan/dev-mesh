import test from "node:test";
import assert from "node:assert/strict";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { Markdown, stripAnsi } from "./Markdown.tsx";
import { Overview } from "./Overview.tsx";
import { SessionPanel } from "./SessionPanel.tsx";
import {
  defaultModeFor,
  issueStatus,
  sendDisabledReason,
  type Session,
} from "./workspace.ts";

const baseSession: Session = {
  session_id: "session-abc12345",
  work_id: "work-1",
  project_id: "project-1",
  runtime_instance_id: "runtime-1",
  run_epoch: "1",
  runtime: "codex",
  state: "running",
  connection: "online",
  work: { state: "active", claim_generation: 1 },
  capabilities: { steer: true, follow_up: true, prompt: true, interrupt: true },
  remote_control: {
    enabled: true,
    allow_steer: true,
    allow_follow_up: true,
    allow_prompt: true,
    allow_interrupt: true,
  },
  permission_flags: { steer: true, comment: true, interrupt: true },
};

test("Markdown safely escapes HTML/JS and strips terminal ANSI sequences", () => {
  const html = renderToStaticMarkup(
    createElement(Markdown, {
      text: "Hello <script>alert(1)</script> **bold** `code` [bad](javascript:alert(1)) [ok](https://example.com)\n\n```ts\nconst x = 1;\n```",
    }),
  );
  assert.doesNotMatch(html, /<script>/);
  assert.doesNotMatch(html, /href="javascript:/);
  assert.match(html, /&lt;script&gt;/);
  assert.match(html, /<strong>bold<\/strong>/);
  assert.match(html, /https:\/\/example\.com/);
  assert.match(html, /const x = 1;/);
  assert.equal(stripAnsi("\u001b[32mPASS\u001b[0m"), "PASS");
});

test("issueStatus reflects running, waiting, review, blocked and closed semantics", () => {
  assert.equal(
    issueStatus({
      canonical_state: "in_progress",
      active_sessions: [{ runtime: "codex", state: "running" }],
    }).tone,
    "running",
  );
  assert.equal(
    issueStatus({
      canonical_state: "in_progress",
      active_sessions: [{ runtime: "pi", state: "waiting_approval" }],
    }).tone,
    "waiting",
  );
  assert.equal(issueStatus({ canonical_state: "review" }).tone, "review");
  assert.equal(issueStatus({ canonical_state: "blocked" }).tone, "blocked");
  assert.equal(issueStatus({ source_state: "closed" }).tone, "closed");
});

test("defaultModeFor and sendDisabledReason provide honest guidance per state", () => {
  assert.equal(defaultModeFor(baseSession, {}), "steer");
  assert.equal(
    defaultModeFor({ ...baseSession, state: "idle" }, {}),
    "prompt",
  );
  assert.equal(
    defaultModeFor(
      { ...baseSession, work: { state: "completed" } },
      { permission_flags: { comment: true } },
    ),
    "comment",
  );
  assert.match(
    sendDisabledReason("steer", { ...baseSession, connection: "offline" }, {}) ??
      "",
    /离线/,
  );
  assert.match(
    sendDisabledReason(
      "steer",
      { ...baseSession, state: "waiting_approval" },
      {},
    ) ?? "",
    /审批/,
  );
});

test("SessionPanel renders content_policy redaction badges and approval CLI command", () => {
  const html = renderToStaticMarkup(
    createElement(SessionPanel, {
      session: { ...baseSession, state: "waiting_approval" },
      projectName: "mix-oneai",
      events: [
        {
          event_id: "e1",
          sequence: "1",
          type: "message.completed",
          payload: {
            author_kind: "agent",
            text: "已完成接口分析。",
            content_policy: { redacted: true, truncated: true },
          },
        },
        {
          event_id: "e2",
          sequence: "2",
          type: "approval.requested",
          payload: { kind: "approval" },
        },
      ],
      commands: [],
      comments: [],
      issue: {
        id: "i1",
        title: "模型路由优化",
        issue_ref: { issue_number: 86 },
        role: "backend",
      },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.match(html, /mix-oneai/);
  assert.match(html, /#86/);
  assert.match(html, /已脱敏/);
  assert.match(html, /内容已截断/);
  assert.match(html, /devmesh approvals list/);
});

test("Overview summarizes running tasks, review queue, and devices", () => {
  const html = renderToStaticMarkup(
    createElement(Overview, {
      directory: [
        {
          project: { id: "p1", name: "mix-oneai" },
          issues: [
            {
              id: "i1",
              title: "模型路由优化",
              issue_ref: { issue_number: 86 },
              role: "backend",
              active_sessions: [{ runtime: "pi", state: "running" }],
            },
          ],
        },
      ],
      devices: [{ id: "dev-1234567890ab", name: "开发笔记本", online: true }],
      starredIds: new Set(["i1"]),
      onSelectIssue: () => {},
    }),
  );
  assert.match(html, /工作台概览/);
  assert.match(html, /模型路由优化/);
  assert.match(html, /开发笔记本/);
});

test("terminated session never renders a spinning tool or stale approval CTA", () => {
  const html = renderToStaticMarkup(
    createElement(SessionPanel, {
      session: {
        ...baseSession,
        state: "ended",
        work: { state: "blocked", claim_generation: 1 },
      },
      events: [
        {
          event_id: "e1",
          sequence: "1",
          occurred_at: "2026-11-26T07:44:25Z",
          type: "tool.completed",
          payload: {
            tool_call_id: "t1",
            tool: "commandExecution",
            outcome: "success",
            exit_code: 0,
          },
        },
        {
          event_id: "e2",
          sequence: "2",
          occurred_at: "2026-11-26T07:44:40Z",
          type: "tool.started",
          payload: { tool_call_id: "t2", tool: "commandExecution" },
        },
        {
          event_id: "e3",
          sequence: "3",
          occurred_at: "2026-11-26T07:44:40Z",
          type: "approval.requested",
          payload: { kind: "approval" },
        },
        {
          event_id: "e4",
          sequence: "4",
          occurred_at: "2026-11-26T07:59:41Z",
          type: "work.state",
          payload: {
            state: "blocked",
            reason: "waiting_approval timeout or missing delivery",
          },
        },
      ],
      commands: [],
      comments: [],
      issue: { id: "i1", title: "Blocked task" },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.doesNotMatch(html, /dm-spin/);
  assert.doesNotMatch(html, /正在运行终端命令/);
  assert.doesNotMatch(html, /devmesh approvals list/);
  assert.match(html, /终端命令未完成/);
  assert.match(html, /工作受阻/);
});

test("SessionPanel displays account name and initials for human messages", () => {
  const html = renderToStaticMarkup(
    createElement(SessionPanel, {
      session: baseSession,
      currentUserName: "admin",
      events: [],
      commands: [],
      comments: [
        { comment_id: "c1", message: "为什么受阻", sender_name: "zhangsan" },
        { comment_id: "c2", message: "我来跟进" },
      ],
      issue: { id: "i1", title: "Task" },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.match(html, /zhangsan/);
  assert.match(html, />ZH</);
  assert.match(html, /admin/);
  assert.match(html, />AD</);
  assert.doesNotMatch(html, />人</);
  assert.doesNotMatch(html, />人工</);
});
