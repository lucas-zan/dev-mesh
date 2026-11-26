import test from "node:test";
import assert from "node:assert/strict";
import { conversationLog, currentWorkAction, workLog } from "./work-log.ts";
import type { WorkEvent } from "./workspace.ts";

function event(
  sequence: number,
  type: string,
  payload: WorkEvent["payload"],
): WorkEvent {
  return {
    event_id: String(sequence),
    sequence: String(sequence),
    occurred_at: "2026-11-26T07:59:41Z",
    type,
    payload,
  };
}

test("turns blocked work into a readable outcome and omits repeated internal state", () => {
  const rows = workLog([
    event(17, "work.state", { state: "finalizing", previous: "active" }),
    event(18, "session.state", {
      state: "ended",
      runtime_instance_id: "hidden",
    }),
    event(19, "work.state", {
      state: "blocked",
      reason: "waiting_approval timeout or missing delivery",
    }),
    event(20, "x.native", { details: { token: "private" } }),
  ]);
  assert.deepEqual(
    rows.map(({ id, title, text }) => ({ id, title, text })),
    [{ id: "19", title: "工作受阻", text: "等待本机审批超时。" }],
  );
  assert.equal(rows[0].time, "2026-11-26T07:59:41Z");
});

test("uses the tool completion when available and shows actionable failure metadata", () => {
  const rows = workLog([
    event(1, "tool.started", { tool_call_id: "a", tool: "commandExecution" }),
    event(2, "tool.started", { tool_call_id: "b", tool: "work_push" }),
    event(3, "tool.completed", {
      tool_call_id: "a",
      tool: "commandExecution",
      outcome: "failed",
      exit_code: 1,
      duration_ms: 1500,
    }),
  ]);
  assert.deepEqual(
    rows.map((row) => row.id),
    ["2", "3"],
  );
  assert.equal(rows[0].title, "正在推送分支");
  assert.equal(rows[1].title, "终端命令失败");
  assert.equal(rows[1].text, "退出码 1 · 1.5 秒");
});

test("retains visible user and agent messages and distinguishes input from approval", () => {
  const rows = workLog([
    event(1, "message.completed", {
      author_kind: "user",
      text: "Update README",
    }),
    event(2, "message.completed", {
      author_kind: "agent",
      text: "Tests passed.\nReady to push.",
    }),
    event(3, "approval.requested", { kind: "input" }),
    event(4, "message.completed", {
      author_kind: "agent",
      text: { nested: true },
    }),
    event(5, "approval.requested", { kind: "approval" }),
  ]);
  assert.deepEqual(
    rows.map((row) => row.title),
    ["你", "Agent", "等待本机输入", "等待本机审批"],
  );
  assert.equal(rows[1].text, "Tests passed.\nReady to push.");
});

test("omits Worker launch instructions from historical user messages", () => {
  const rows = workLog([
    event(1, "message.completed", {
      author_kind: "user",
      text: 'Use DevMesh collaboration skill at [redacted]. Complete this Issue. Work context: {"work_id":"internal"}. Requirements:\n{"required_role":"product"}',
    }),
    event(2, "message.completed", {
      author_kind: "user",
      text: "Check the public configuration sample.",
    }),
  ]);
  assert.deepEqual(
    rows.map((row) => row.text),
    ["Check the public configuration sample."],
  );
});

test("merges human comments with Agent messages in chronological order", () => {
  const first = event(1, "message.completed", {
    author_kind: "agent",
    text: "I inspected the repository.",
  });
  const last = {
    ...event(2, "message.completed", {
      author_kind: "user",
      text: "Keep the README changes.",
    }),
    occurred_at: "2026-11-26T08:01:00Z",
  };
  const rows = conversationLog(
    [first, last],
    [
      {
        comment_id: "human",
        message: "为什么受阻",
        created_at: "2026-11-26T08:00:00Z",
      },
      {
        comment_id: "note",
        message: "Codex 处理记录（会话外补记）。已验证远端提交。",
        created_at: "2026-11-26T08:02:00Z",
      },
    ],
  );
  assert.deepEqual(
    rows.map((row) => [row.id, row.kind]),
    [
      ["1", "agent"],
      ["comment:human", "user"],
      ["2", "user"],
      ["comment:note", "note"],
    ],
  );
  assert.equal(rows[1].text, "为什么受阻");
});

test("folds consecutive tool events while keeping approval and blocked visible", () => {
  const rows = conversationLog(
    [
      event(1, "tool.completed", {
        tool: "commandExecution",
        outcome: "success",
      }),
      event(2, "tool.completed", { tool: "work_run_test", outcome: "success" }),
      event(3, "approval.requested", { kind: "approval" }),
      event(4, "work.state", {
        state: "blocked",
        reason: "waiting_approval timeout or missing delivery",
      }),
    ],
    [],
  );
  assert.deepEqual(
    rows.map((row) => row.title),
    ["工作过程", "等待本机审批", "工作受阻"],
  );
  assert.deepEqual(
    rows[0].steps?.map((step) => step.title),
    ["终端命令已完成", "测试已完成"],
  );
});

test("shows delivered command detail and uses the actual running tool as progress", () => {
  const rows = conversationLog(
    [
      event(1, "tool.completed", {
        tool_call_id: "a",
        tool: "commandExecution",
        outcome: "success",
        summary: "读取 src/lib.rs",
        exit_code: 0,
      }),
      event(2, "tool.started", {
        tool_call_id: "b",
        tool: "commandExecution",
        summary: "cargo test",
      }),
    ],
    [],
    { sessionRunning: true },
  );
  assert.equal(rows[0].steps?.[0].detail, "读取 src/lib.rs");
  assert.equal(rows[0].steps?.[0].text, "退出码 0");
  assert.equal(currentWorkAction(rows), "cargo test");
  assert.equal(
    currentWorkAction(
      conversationLog(
        [
          event(3, "tool.completed", {
            tool_call_id: "b",
            tool: "commandExecution",
            summary: "cargo test",
            outcome: "success",
          }),
        ],
        [],
      ),
    ),
    undefined,
  );
});

test("lists the Agent's visible plan and uses its latest active step between commands", () => {
  const plan = (id: number, status: string) =>
    event(id, "plan.updated", {
      plan_id: "turn",
      steps: [
        { id: "1", status: "done", text: "检查仓库" },
        { id: "2", status, text: "补回归测试" },
      ],
    });
  const ongoing = conversationLog([plan(1, "running")], [], {
    sessionRunning: true,
  });
  assert.equal(ongoing[0].title, "执行计划");
  assert.match(ongoing[0].text ?? "", /已完成 · 检查仓库/);
  assert.equal(currentWorkAction(ongoing), "补回归测试");
  assert.equal(
    currentWorkAction(
      conversationLog([plan(1, "running"), plan(2, "done")], [], {
        sessionRunning: true,
      }),
    ),
    undefined,
  );
});
