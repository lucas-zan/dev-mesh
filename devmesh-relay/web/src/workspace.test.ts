import test from "node:test";
import assert from "node:assert/strict";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { canSend, mergeEvents, buildCommand } from "./workspace.ts";
import { SessionPanel } from "./SessionPanel.tsx";

const session = {
  session_id: "session-1",
  work_id: "work-1",
  project_id: "project-1",
  runtime_instance_id: "runtime-1",
  run_epoch: "2",
  runtime: "codex",
  state: "running",
  connection: "online",
  work: { state: "active", claim_generation: 1 },
  capabilities: { steer: true, follow_up: true, prompt: true },
  remote_control: {
    enabled: true,
    allow_steer: true,
    allow_follow_up: true,
    allow_prompt: true,
  },
  permission_flags: { steer: true, comment: true },
};
test("terminal, offline and native-incompatible sessions cannot receive runtime commands", () => {
  assert.equal(canSend("steer", session), true);
  assert.equal(canSend("prompt", session), false);
  assert.equal(
    canSend("prompt", {
      ...session,
      state: "idle",
      work: { ...session.work, state: "completed" },
    }),
    false,
  );
  assert.equal(canSend("steer", { ...session, connection: "offline" }), false);
  assert.equal(
    canSend("steer", { ...session, capabilities: { steer: false } }),
    false,
  );
  assert.equal(
    canSend("comment", { ...session, work: { state: "completed" } }),
    true,
  );
});
test("events deduplicate by event id and sort full precision sequence strings", () => {
  const events = mergeEvents(
    [{ event_id: "b", sequence: "9007199254740993" }],
    [
      { event_id: "a", sequence: "9007199254740992" },
      { event_id: "b", sequence: "9007199254740993" },
    ],
  );
  assert.deepEqual(
    events.map((e) => e.event_id),
    ["a", "b"],
  );
});
test("commands target exactly the selected session instance and claim", () => {
  const command = buildCommand("steer", "preserve API", session);
  assert.equal(command.target.session_id, "session-1");
  assert.equal(command.target.expected_run_epoch, "2");
  assert.equal(command.target.claim_generation, 1);
});
test("approval waiting and unclaimed issues show honest state and disabled controls", () => {
  const html = renderToStaticMarkup(
    createElement(SessionPanel, {
      session: { ...session, state: "waiting_approval" },
      events: [],
      commands: [],
      comments: [],
      issue: { id: "issue-1", title: "Task" },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.match(html, /本机/);
  assert.match(html, /disabled/);
  assert.doesNotMatch(html, /正在执行/);
  const empty = renderToStaticMarkup(
    createElement(SessionPanel, {
      session: null,
      events: [],
      commands: [],
      comments: [],
      issue: { id: "issue-2", title: "Unclaimed" },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.match(empty, /等待/);
  assert.doesNotMatch(empty, /Session 01/);
});

test("an unclaimed Issue can receive a historical comment when permitted", () => {
  const html = renderToStaticMarkup(
    createElement(SessionPanel, {
      session: null,
      events: [],
      commands: [],
      comments: [],
      issue: {
        id: "issue",
        title: "Waiting",
        permission_flags: { comment: true },
      },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.match(html, /评论/);
  assert.equal(
    canSend("interrupt", {
      ...session,
      state: "waiting_approval",
      capabilities: { interrupt: true },
      remote_control: { enabled: true, allow_interrupt: true },
      permission_flags: { interrupt: true },
    }),
    true,
  );
});

test("an already closed Issue without a Session does not suggest work or an offline device", () => {
  const html = renderToStaticMarkup(
    createElement(SessionPanel, {
      session: null,
      events: [],
      commands: [],
      comments: [],
      issue: { title: "Closed", source_state: "closed" },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.match(html, /已关闭/);
  assert.match(html, /尚无会话/);
  assert.doesNotMatch(html, /设备未连接|等待 Agent 认领/);
});

test("record view displays runtime Agent messages and human comments with different avatars", () => {
  const html = renderToStaticMarkup(
    createElement(SessionPanel, {
      session,
      events: [
        {
          event_id: "reply",
          sequence: "1",
          type: "message.completed",
          payload: { author_kind: "agent", text: "README checked." },
        },
      ],
      commands: [],
      comments: [{ comment_id: "question", message: "为什么受阻" }],
      issue: { id: "issue", title: "Task" },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.match(html, /conversation-agent[^]*conversation-avatar[^>]*>AI</);
  assert.match(html, /conversation-user[^]*conversation-avatar[^>]*>人</);
  assert.match(html, /README checked/);
  assert.match(html, /为什么受阻/);
});

test("a live conversation renders the active command and offers direction correction", () => {
  const html = renderToStaticMarkup(
    createElement(SessionPanel, {
      session,
      events: [
        {
          event_id: "tool",
          sequence: "1",
          type: "tool.started",
          payload: {
            tool_call_id: "t",
            tool: "commandExecution",
            summary: "cargo test",
          },
        },
      ],
      commands: [],
      comments: [],
      issue: { id: "issue", title: "Task" },
      onSend: async () => {},
      loading: false,
    }),
  );
  assert.match(html, /正在执行 · cargo test/);
  assert.match(html, /纠正方向 · 当前任务/);
  assert.doesNotMatch(html, /退出码 0/);
});
