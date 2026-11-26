import { RelayClient } from "@devmesh/protocol/client";
export interface RecordData {
  [key: string]: unknown;
}
export function isClosedIssue(issue: RecordData): boolean {
  return issue.source_state === "closed" || issue.canonical_state === "done";
}

export interface Session {
  session_id: string;
  work_id: string;
  project_id: string;
  runtime_instance_id: string;
  run_epoch: string;
  runtime?: string;
  runtime_version?: string;
  adapter_version?: string;
  device_id?: string;
  state: string;
  connection: string;
  work: {
    state: string;
    claim_generation?: number;
    role?: string;
    started_at?: string;
    claim_id?: string;
  };
  capabilities: Record<string, boolean>;
  remote_control: Record<string, boolean>;
  permission_flags: Record<string, boolean>;
}
export interface WorkEvent extends RecordData {
  event_id: string;
  sequence: string;
  type?: string;
  payload?: RecordData;
}

export type IssueVisualTone =
  | "running"
  | "waiting"
  | "review"
  | "blocked"
  | "closed"
  | "idle";

export interface IssueVisualStatus {
  tone: IssueVisualTone;
  label: string;
  priority: number;
  runtime?: string;
  activeCount: number;
}

const terminal = new Set([
  "completed",
  "blocked",
  "failed",
  "cancelled",
  "recovery_required",
  "finalizing",
]);

export function runtimeLabel(runtime?: unknown): string | undefined {
  if (typeof runtime !== "string" || !runtime.trim()) return undefined;
  const normalized = runtime.toLowerCase();
  if (normalized === "codex") return "Codex";
  if (normalized === "pi") return "Pi";
  if (normalized === "pi_durable" || normalized === "pi-durable")
    return "Pi Durable";
  return runtime;
}

export function roleLabel(role?: unknown): string {
  if (typeof role !== "string" || !role.trim()) return "未分配角色";
  const map: Record<string, string> = {
    backend: "Backend",
    frontend: "Frontend",
    product: "Product",
    qa: "QA",
  };
  return map[role.toLowerCase()] ?? role;
}

export function issueNumber(issue: RecordData): number | undefined {
  const ref = issue.issue_ref as RecordData | undefined;
  if (typeof ref?.issue_number === "number") return ref.issue_number;
  if (typeof issue.number === "number") return issue.number;
  return undefined;
}

export function issueStatus(issue: RecordData): IssueVisualStatus {
  const active = Array.isArray(issue.active_sessions)
    ? (issue.active_sessions as RecordData[])
    : [];
  const runtime = runtimeLabel(active.at(-1)?.runtime ?? issue.runtime);
  const activeCount = active.length;
  const hasWaiting = active.some((s) =>
    ["waiting_approval", "waiting_input"].includes(String(s.state ?? "")),
  );
  const hasRunning = active.some((s) =>
    ["running", "thinking", "tool_running", "starting"].includes(
      String(s.state ?? ""),
    ),
  );
  const canonical = String(issue.canonical_state ?? "");

  if (hasWaiting) {
    return {
      tone: "waiting",
      label: "等待本机处理",
      priority: 0,
      runtime,
      activeCount,
    };
  }
  if (hasRunning) {
    return {
      tone: "running",
      label: "运行中",
      priority: 1,
      runtime,
      activeCount,
    };
  }
  if (canonical === "blocked" || canonical === "failed") {
    return {
      tone: "blocked",
      label: canonical === "blocked" ? "受阻" : "失败",
      priority: 2,
      runtime,
      activeCount,
    };
  }
  if (canonical === "review") {
    return {
      tone: "review",
      label: "待审核",
      priority: 3,
      runtime,
      activeCount,
    };
  }
  if (isClosedIssue(issue)) {
    return {
      tone: "closed",
      label: "已完成",
      priority: 5,
      runtime,
      activeCount,
    };
  }
  if (canonical === "claimed" || canonical === "in_progress") {
    return {
      tone: "idle",
      label: "已认领",
      priority: 4,
      runtime,
      activeCount,
    };
  }
  return {
    tone: "idle",
    label: canonical === "ready" ? "待认领" : canonical || "状态未知",
    priority: 4,
    runtime,
    activeCount,
  };
}

export function canSend(mode: string, session: Session | null): boolean {
  if (!session) return false;
  if (mode === "comment") return session.permission_flags.comment === true;
  if (terminal.has(session.work.state) || session.connection !== "online")
    return false;
  const permission = mode === "interrupt" ? "interrupt" : "steer";
  if (
    !session.permission_flags[permission] ||
    !session.remote_control.enabled ||
    !session.capabilities[mode]
  )
    return false;
  if (!session.remote_control[`allow_${mode}`]) return false;
  if (mode === "prompt") return session.state === "idle";
  if (mode === "interrupt")
    return ["running", "waiting_approval", "waiting_input"].includes(
      session.state,
    );
  return ["running", "thinking", "tool_running"].includes(session.state);
}

export function sendDisabledReason(
  mode: string,
  session: Session | null,
  issue: RecordData,
): string | null {
  if (mode === "comment") {
    return canComment(issue) || canSend("comment", session)
      ? null
      : "当前账号无评论权限";
  }
  if (!session) return "尚无运行会话";
  if (terminal.has(session.work.state)) return "Work 已结束，会话只读";
  if (session.connection !== "online") return "执行设备离线";
  const permission = mode === "interrupt" ? "interrupt" : "steer";
  if (!session.permission_flags[permission]) return "缺少远程控制权限";
  if (!session.remote_control.enabled) return "本机未开启远程控制";
  if (!session.capabilities[mode]) return "当前 Runtime 不支持此操作";
  if (!session.remote_control[`allow_${mode}`]) return "本机策略已禁用此模式";
  if (mode === "prompt" && session.state !== "idle")
    return "仅空闲会话支持 Prompt";
  if (
    mode === "interrupt" &&
    !["running", "waiting_approval", "waiting_input"].includes(session.state)
  )
    return "当前无运行中任务";
  if (
    (mode === "steer" || mode === "follow_up") &&
    !["running", "thinking", "tool_running"].includes(session.state)
  )
    return session.state === "waiting_approval" ||
      session.state === "waiting_input"
      ? "等待本机审批/输入期间不可 Steer"
      : "仅运行中会话支持此模式";
  return null;
}

export function defaultModeFor(
  session: Session | null,
  issue: RecordData,
): string {
  if (!session) return "comment";
  if (terminal.has(session.work.state)) return "comment";
  if (canSend("steer", session)) return "steer";
  if (canSend("prompt", session)) return "prompt";
  if (canSend("follow_up", session)) return "follow_up";
  if (
    session.state === "waiting_approval" ||
    session.state === "waiting_input"
  ) {
    return "steer";
  }
  if (canComment(issue) || canSend("comment", session)) return "comment";
  return "steer";
}

export function mergeEvents(
  previous: WorkEvent[],
  incoming: WorkEvent[],
): WorkEvent[] {
  const records = new Map(previous.map((event) => [event.event_id, event]));
  for (const event of incoming) records.set(event.event_id, event);
  return [...records.values()].sort((a, b) =>
    BigInt(a.sequence) < BigInt(b.sequence)
      ? -1
      : BigInt(a.sequence) > BigInt(b.sequence)
        ? 1
        : 0,
  );
}
export function buildCommand(type: string, message: string, session: Session) {
  return {
    type,
    message,
    ttl_seconds: 60,
    target: {
      project_id: session.project_id,
      work_id: session.work_id,
      session_id: session.session_id,
      runtime_instance_id: session.runtime_instance_id,
      expected_run_epoch: session.run_epoch,
      claim_generation: session.work.claim_generation,
    },
  };
}
const client = new RelayClient();
export function api<T>(path: string, options: RequestInit = {}): Promise<T> {
  return client.request<T>(path, options);
}

export function canComment(issue: RecordData): boolean {
  return (
    (issue.permission_flags as Record<string, boolean> | undefined)?.comment ===
    true
  );
}
