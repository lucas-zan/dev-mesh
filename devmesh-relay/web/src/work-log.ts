import type { RecordData, WorkEvent } from "./workspace.ts";

export interface ContentPolicy {
  redacted?: boolean;
  truncated?: boolean;
  omitted?: boolean;
}

export interface LogOptions {
  /** Whether the session is currently actively executing tools right now. */
  sessionRunning?: boolean;
  /** Whether the session/work has already reached a terminal state. */
  sessionEnded?: boolean;
  /** Whether the session is currently paused waiting for local approval/input. */
  sessionWaiting?: boolean;
}

interface Entry {
  title: string;
  kind: "agent" | "user" | "status" | "tool" | "warning" | "note" | "activity";
  text?: string;
  detail?: string;
  plan?: { current?: string };
  output?: string;
  policy?: ContentPolicy;
  running?: boolean;
  interrupted?: boolean;
  toolCategory?: "command" | "file" | "test" | "delivery" | "other";
  outcome?: string;
  approvalKind?: "approval" | "input";
  approvalActive?: boolean;
  commitSha?: string;
  source?: string;
  author?: string;
}

export interface LogEntry extends Entry {
  id: string;
  time?: string;
  steps?: LogEntry[];
}

const workStates = new Map([
  ["active", "工作已启动"],
  ["completed", "工作已完成"],
  ["blocked", "工作受阻"],
  ["failed", "工作失败"],
  ["cancelled", "工作已取消"],
  ["recovery_required", "工作等待恢复"],
]);

const terminalWorkStates = new Set([
  "completed",
  "blocked",
  "failed",
  "cancelled",
  "recovery_required",
]);

const terminalSessionStates = new Set(["ended", "failed", "cancelled"]);

const reasons = new Map([
  ["waiting_approval timeout or missing delivery", "等待本机审批超时。"],
  ["waiting_input timeout or missing delivery", "等待本机输入超时。"],
  ["idle timeout or missing delivery", "Agent 停止活动，未提交交付结果。"],
  ["delivery verified", "交付证据已验证。"],
]);

const fileActions = new Map([
  ["added", "新增文件"],
  ["modified", "修改文件"],
  ["deleted", "删除文件"],
  ["renamed", "重命名文件"],
]);

const tools = new Map<string, [string, string, Entry["toolCategory"]]>([
  ["commandExecution", ["正在运行终端命令", "终端命令", "command"]],
  ["bash", ["正在运行终端命令", "终端命令", "command"]],
  ["read", ["正在读取文件", "读取文件", "file"]],
  ["edit", ["正在编辑文件", "编辑文件", "file"]],
  ["write", ["正在写入文件", "写入文件", "file"]],
  ["work_run_test", ["正在运行测试", "测试", "test"]],
  ["work_push", ["正在推送分支", "推送分支", "delivery"]],
  [
    "work_create_pr",
    ["正在创建 Pull Request", "创建 Pull Request", "delivery"],
  ],
  ["work_report_delivery", ["正在提交交付结果", "提交交付结果", "delivery"]],
  ["mcpToolCall", ["正在调用 MCP 工具", "MCP 工具调用", "other"]],
  ["dynamicToolCall", ["正在调用工具", "工具调用", "other"]],
]);

const outcomes = new Map([
  ["success", "已完成"],
  ["failed", "失败"],
  ["cancelled", "已取消"],
]);

function text(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function readPolicy(payload: RecordData): ContentPolicy | undefined {
  const raw = payload.content_policy as RecordData | undefined;
  if (!raw || typeof raw !== "object") return undefined;
  const policy: ContentPolicy = {
    redacted: raw.redacted === true,
    truncated: raw.truncated === true,
    omitted: raw.omitted === true,
  };
  return policy.redacted || policy.truncated || policy.omitted
    ? policy
    : undefined;
}

function isWorkerPrompt(message: string): boolean {
  return (
    message.startsWith("Use DevMesh collaboration skill at ") &&
    message.includes("Work context: ") &&
    message.includes("Requirements:")
  );
}

function isTerminalEvent(event: WorkEvent): boolean {
  const payload = event.payload ?? {};
  if (
    event.type === "work.state" &&
    terminalWorkStates.has(text(payload.state) ?? "")
  ) {
    return true;
  }
  if (
    event.type === "session.state" &&
    terminalSessionStates.has(text(payload.state) ?? "")
  ) {
    return true;
  }
  return event.type === "runtime.disconnected";
}

function toolEntry(
  payload: RecordData,
  complete: boolean,
  endedAfter: boolean,
  waitingAfter: boolean,
): Entry {
  const tool = text(payload.tool);
  const [runningTitle, finished, toolCategory] = tools.get(tool ?? "") ?? [
    tool ? `正在运行 ${tool}` : "正在运行工具",
    tool ?? "工具调用",
    "other",
  ];
  const outcome = text(payload.outcome) ?? "";
  const summary = text(payload.summary);
  const detail =
    summary && summary !== "Tool started" && summary !== "Tool finished"
      ? summary
      : undefined;
  const metadata = [];
  if (typeof payload.exit_code === "number")
    metadata.push(`退出码 ${payload.exit_code}`);
  if (typeof payload.duration_ms === "number" && payload.duration_ms > 0)
    metadata.push(`${Number((payload.duration_ms / 1000).toFixed(2))} 秒`);

  if (!complete && endedAfter) {
    return {
      title: `${finished}未完成`,
      kind: "tool",
      text: metadata.join(" · ") || "会话已终止",
      detail,
      output: text(payload.output),
      policy: readPolicy(payload),
      running: false,
      interrupted: true,
      toolCategory,
    };
  }
  if (!complete && waitingAfter) {
    return {
      title: `${finished}（等待本机确认）`,
      kind: "tool",
      text: metadata.join(" · ") || undefined,
      detail,
      output: text(payload.output),
      policy: readPolicy(payload),
      running: false,
      toolCategory,
    };
  }

  return {
    title: complete
      ? finished + (outcomes.get(outcome) ?? "已结束")
      : runningTitle,
    kind: outcome === "failed" ? "warning" : "tool",
    text: metadata.join(" · ") || undefined,
    detail,
    output: text(payload.output),
    policy: readPolicy(payload),
    running: !complete,
    toolCategory,
    outcome: outcome || undefined,
  };
}

export function currentWorkAction(rows: LogEntry[]): string | undefined {
  const active = rows
    .flatMap((row) => row.steps ?? [row])
    .reverse()
    .find((row) => row.running)?.detail;
  return active ?? [...rows].reverse().find((row) => row.plan)?.plan?.current;
}

const planStatus = new Map([
  ["pending", "待处理"],
  ["running", "进行中"],
  ["done", "已完成"],
  ["blocked", "受阻"],
]);

function planEntry(payload: RecordData): Entry | undefined {
  if (!Array.isArray(payload.steps)) return;
  const steps = (payload.steps as RecordData[]).filter((step) =>
    text(step?.text),
  );
  if (!steps.length) return;
  return {
    kind: "status",
    title: "执行计划",
    text: steps
      .map(
        (step) =>
          `${planStatus.get(String(step.status)) ?? "未标记"} · ${step.text}`,
      )
      .join("\n"),
    plan: {
      current: text(steps.find((step) => step.status === "running")?.text),
    },
  };
}

function workEntry(payload: RecordData): Entry | undefined {
  const state = text(payload.state) ?? "";
  const title = workStates.get(state);
  if (!title) return;
  const reason = text(payload.reason);
  return {
    title,
    kind: ["blocked", "failed", "recovery_required"].includes(state)
      ? "warning"
      : "status",
    text: reason ? (reasons.get(reason) ?? reason) : undefined,
  };
}

function summarize(
  type: string | undefined,
  payload: RecordData,
  context: {
    endedAfter: boolean;
    waitingAfter: boolean;
    approvalActive: boolean;
  },
): Entry | undefined {
  switch (type) {
    case "message.completed": {
      const message = text(payload.text);
      if (!message) return;
      const policy = readPolicy(payload);
      if (payload.author_kind === "user" && !isWorkerPrompt(message))
        return { title: "你", kind: "user", text: message, policy };
      if (payload.author_kind === "agent")
        return { title: "Agent", kind: "agent", text: message, policy };
      return;
    }
    case "work.state":
      return workEntry(payload);
    case "plan.updated":
      return planEntry(payload);
    case "tool.started":
    case "tool.completed":
      return toolEntry(
        payload,
        type === "tool.completed",
        context.endedAfter,
        context.waitingAfter,
      );
    case "approval.requested": {
      const isInput = payload.kind === "input";
      return {
        title: isInput ? "等待本机输入" : "等待本机审批",
        kind: "warning",
        approvalKind: isInput ? "input" : "approval",
        approvalActive: context.approvalActive,
      };
    }
    case "approval.resolved":
      return {
        title:
          payload.kind === "input" ? "本机输入请求已处理" : "本机审批已处理",
        kind: "status",
      };
    case "file.changed": {
      const path = text(payload.path);
      const action = fileActions.get(text(payload.kind) ?? "");
      if (path && action)
        return {
          title: action,
          kind: "tool",
          text: path,
          toolCategory: "file",
        };
      return;
    }
    case "delivery.reported": {
      const sha = text(payload.candidate_sha);
      return {
        title: "已提交交付结果",
        kind: "status",
        text: sha ? `提交 ${sha.slice(0, 12)}` : undefined,
        toolCategory: "delivery",
        commitSha: sha,
      };
    }
    case "capture.gap":
      return { title: "部分日志缺失", kind: "warning" };
    case "runtime.disconnected":
      return { title: "Agent 已断开连接", kind: "warning" };
  }
}

export function workLog(
  events: WorkEvent[],
  options: LogOptions = {},
): LogEntry[] {
  const finished = new Set(
    events.flatMap((event) => {
      const id = text(event.payload?.tool_call_id);
      return event.type === "tool.completed" && id ? [id] : [];
    }),
  );
  let lastTerminalIndex = -1;
  let lastApprovalIndex = -1;
  let lastResolvedIndex = -1;
  for (let i = 0; i < events.length; i++) {
    if (isTerminalEvent(events[i])) lastTerminalIndex = i;
    if (events[i].type === "approval.requested") lastApprovalIndex = i;
    if (events[i].type === "approval.resolved") lastResolvedIndex = i;
  }

  return events.flatMap((event, index) => {
    const payload = event.payload ?? {};
    const call = text(payload.tool_call_id);
    if (event.type === "tool.started" && call && finished.has(call)) return [];

    const endedAfter =
      options.sessionEnded === true ||
      lastTerminalIndex > index ||
      (options.sessionRunning === false && options.sessionWaiting !== true);
    const waitingAfter =
      !endedAfter &&
      (options.sessionWaiting === true ||
        (lastApprovalIndex > index && lastResolvedIndex < lastApprovalIndex));
    const approvalActive =
      event.type === "approval.requested" &&
      !endedAfter &&
      index === lastApprovalIndex &&
      lastResolvedIndex < index &&
      options.sessionEnded !== true;

    const entry = summarize(event.type, payload, {
      endedAfter,
      waitingAfter,
      approvalActive,
    });
    return entry
      ? [{ ...entry, id: event.event_id, time: text(event.occurred_at) }]
      : [];
  });
}

const legacyNote = "Codex 处理记录（会话外补记）。";

function commentEntry(comment: RecordData, index: number): LogEntry[] {
  const message = text(comment.message);
  if (!message) return [];
  const supplemental = message.startsWith(legacyNote);
  const author = supplemental
    ? undefined
    : (text(comment.sender_name) ??
      text(comment.author_name) ??
      text(comment.username));
  return [
    {
      id: `comment:${text(comment.comment_id) ?? index}`,
      kind: supplemental ? "note" : "user",
      title: supplemental ? "Codex 工作补记" : (author ?? "人工"),
      author,
      source: supplemental ? undefined : "评论",
      time: text(comment.created_at),
      text: supplemental ? message.slice(legacyNote.length) : message,
    },
  ];
}

function chronological(a: LogEntry, b: LogEntry): number {
  return (Date.parse(a.time ?? "") || 0) - (Date.parse(b.time ?? "") || 0);
}

export function conversationLog(
  events: WorkEvent[],
  comments: RecordData[],
  options: LogOptions = {},
): LogEntry[] {
  const entries = [
    ...workLog(events, options),
    ...comments.flatMap(commentEntry),
  ].sort(chronological);
  const rows: LogEntry[] = [];
  for (const entry of entries) {
    if (entry.kind !== "tool") {
      rows.push(entry);
      continue;
    }
    const previous = rows.at(-1);
    if (previous?.steps) {
      previous.steps.push(entry);
      previous.time = entry.time;
      if (entry.running) previous.running = true;
    } else {
      rows.push({
        id: entry.id,
        kind: "activity",
        title: "工作过程",
        time: entry.time,
        running: entry.running,
        steps: [entry],
      });
    }
  }
  return rows;
}
