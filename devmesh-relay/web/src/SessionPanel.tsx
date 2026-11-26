import { useState, useRef, useEffect, useMemo } from "react";
import {
  AlertTriangleIcon,
  ArrowUpIcon,
  CheckCircleIcon,
  CheckIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CopyIcon,
  ExternalLinkIcon,
  FileCodeIcon,
  GitCommitIcon,
  RadioIcon,
  SearchIcon,
  ShieldAlertIcon,
  SpinnerIcon,
  StopCircleIcon,
  TerminalIcon,
} from "./Icons.tsx";
import { Markdown, stripAnsi } from "./Markdown.tsx";
import { VirtualEvents } from "./VirtualEvents.tsx";
import {
  conversationLog,
  currentWorkAction,
  type ContentPolicy,
  type LogEntry,
} from "./work-log.ts";
import {
  canSend,
  canComment,
  defaultModeFor,
  isClosedIssue,
  issueNumber,
  roleLabel,
  runtimeLabel,
  sendDisabledReason,
  type Session,
  type WorkEvent,
  type RecordData,
} from "./workspace.ts";

interface Props {
  session: Session | null;
  sessions?: Session[];
  sessionId?: string;
  onSelectSession?: (sessionId: string) => void;
  projectName?: string;
  currentUserName?: string;
  events: WorkEvent[];
  commands: RecordData[];
  comments: RecordData[];
  issue: RecordData;
  onSend: (mode: string, text: string) => Promise<void>;
  loading: boolean;
  hasEarlier?: boolean;
  onLoadEarlier?: () => Promise<void>;
}

const stateNames: Record<string, string> = {
  starting: "启动中",
  running: "运行中",
  thinking: "思考中",
  tool_running: "执行工具",
  idle: "空闲",
  waiting_approval: "等待本机审批",
  waiting_input: "等待本机输入",
  ended: "已结束",
  failed: "失败",
  cancelled: "已取消",
  unknown: "状态未知",
};

const commandStateNames: Record<string, string> = {
  pending: "待派发",
  dispatched: "已派发",
  accepted: "已接收",
  queued: "已入队",
  applied: "已生效",
  handled: "已处理",
  rejected: "被拒绝",
  failed: "失败",
  expired: "已过期",
  unknown: "状态未知",
};

const terminalWorkStates = new Set([
  "completed",
  "blocked",
  "failed",
  "cancelled",
  "recovery_required",
  "finalizing",
]);

function sessionTone(session: Session | null, closedIssue: boolean): string {
  if (!session) return closedIssue ? "closed" : "idle";
  const workState = session.work?.state;
  if (
    workState === "blocked" ||
    workState === "failed" ||
    workState === "recovery_required"
  )
    return "blocked";
  if (workState === "completed" || workState === "cancelled") return "closed";
  if (
    ["running", "thinking", "tool_running", "starting"].includes(session.state)
  )
    return session.connection === "online" ? "running" : "idle";
  if (session.state === "waiting_approval" || session.state === "waiting_input")
    return "waiting";
  if (session.state === "failed") return "blocked";
  if (session.state === "ended" || session.state === "cancelled")
    return "closed";
  return "idle";
}

function safeExternalUrl(value: unknown): string | null {
  if (typeof value !== "string") return null;
  try {
    const parsed = new URL(value);
    if (parsed.protocol === "http:" || parsed.protocol === "https:") {
      return parsed.href;
    }
    return null;
  } catch {
    return null;
  }
}

function CopyButton({
  value,
  label = "复制",
  copiedLabel = "已复制",
}: {
  value: string;
  label?: string;
  copiedLabel?: string;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="copy-inline-btn"
      onClick={async () => {
        try {
          await navigator.clipboard?.writeText(value);
          setCopied(true);
          setTimeout(() => setCopied(false), 1600);
        } catch {
          setCopied(false);
        }
      }}
    >
      {copied ? <CheckIcon size={12} /> : <CopyIcon size={12} />}
      <span>{copied ? copiedLabel : label}</span>
    </button>
  );
}

export function SessionPanel({
  session,
  sessions = [],
  sessionId,
  onSelectSession,
  projectName,
  currentUserName,
  events,
  commands,
  comments,
  issue,
  onSend,
  loading,
  hasEarlier,
  onLoadEarlier,
}: Props) {
  const [tab, setTab] = useState<"记录" | "原始事件" | "详情">("记录");
  const [mode, setMode] = useState(() => defaultModeFor(session, issue));
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  const [sending, setSending] = useState(false);
  const [confirmInterrupt, setConfirmInterrupt] = useState(false);
  const [eventFilter, setEventFilter] = useState("all");
  const [eventQuery, setEventQuery] = useState("");

  const timeline = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const previousCount = useRef(0);
  const [unseen, setUnseen] = useState(0);

  // Sync sensible default mode when switching issue/session
  const sessionKey = `${String(issue.id ?? "")}:${session?.session_id ?? ""}:${session?.state ?? ""}:${session?.work?.state ?? ""}`;
  useEffect(() => {
    setMode(defaultModeFor(session, issue));
    setConfirmInterrupt(false);
    setError("");
  }, [sessionKey]);

  const sessionEnded =
    session !== null &&
    (terminalWorkStates.has(session.work?.state ?? "") ||
      ["ended", "failed", "cancelled"].includes(session.state));
  const sessionWaiting =
    !sessionEnded &&
    (session?.state === "waiting_approval" ||
      session?.state === "waiting_input");
  const isWorking =
    !sessionEnded &&
    session?.connection === "online" &&
    ["running", "thinking", "tool_running"].includes(session.state);

  const records = useMemo(
    () =>
      conversationLog(events, comments, {
        sessionRunning: isWorking,
        sessionEnded,
        sessionWaiting,
      }),
    [events, comments, isWorking, sessionEnded, sessionWaiting],
  );
  const recordIds = useMemo(
    () => records.map((record) => record.id),
    [records],
  );
  const action = isWorking ? currentWorkAction(records) : undefined;

  useEffect(() => {
    const count = events.length + comments.length;
    const added = Math.max(0, count - previousCount.current);
    previousCount.current = count;
    if (following.current && timeline.current) {
      timeline.current.scrollTop = timeline.current.scrollHeight;
      setUnseen(0);
    } else if (added) setUnseen((c) => c + added);
  }, [events, comments]);

  function follow() {
    following.current = true;
    setUnseen(0);
    if (timeline.current)
      timeline.current.scrollTop = timeline.current.scrollHeight;
  }

  const closedIssue = isClosedIssue(issue);
  const num = issueNumber(issue);
  const giteaUrl = safeExternalUrl(issue.url);
  const runtime = runtimeLabel(session?.runtime);
  const tone = sessionTone(session, closedIssue);
  const canInterruptNow = canSend("interrupt", session);

  const allowed =
    mode === "comment"
      ? canComment(issue) || canSend(mode, session)
      : canSend(mode, session);
  const disabledReason = allowed
    ? null
    : sendDisabledReason(mode, session, issue);

  async function send(targetMode = mode, payloadText = text) {
    setSending(true);
    setError("");
    try {
      await onSend(targetMode, payloadText);
      if (targetMode !== "interrupt") setText("");
      setConfirmInterrupt(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setSending(false);
    }
  }

  const filteredEvents = useMemo(() => {
    const q = eventQuery.trim().toLowerCase();
    return events.filter((event) => {
      const type = String(event.type ?? "");
      if (eventFilter === "message" && !type.startsWith("message."))
        return false;
      if (
        eventFilter === "tool" &&
        !type.startsWith("tool.") &&
        type !== "file.changed"
      )
        return false;
      if (
        eventFilter === "state" &&
        !type.endsWith(".state") &&
        type !== "delivery.reported"
      )
        return false;
      if (eventFilter === "approval" && !type.startsWith("approval."))
        return false;
      if (!q) return true;
      return (
        type.toLowerCase().includes(q) ||
        JSON.stringify(event.payload ?? {})
          .toLowerCase()
          .includes(q)
      );
    });
  }, [events, eventFilter, eventQuery]);

  const activeSessionValue = sessionId ?? session?.session_id ?? "";
  const multiActiveCount = sessions.filter(
    (s) => !["ended", "failed", "cancelled"].includes(s.state),
  ).length;

  return (
    <section className="session-panel">
      <header className="session-header">
        <div className="session-header-top">
          <div className="session-breadcrumb">
            {projectName ? (
              <span className="crumb-project">{projectName}</span>
            ) : null}
            {num !== undefined ? (
              <>
                {projectName ? <span className="crumb-sep">/</span> : null}
                <span className="crumb-number mono">#{num}</span>
              </>
            ) : null}
            {!projectName && num === undefined ? (
              <span className="eyebrow">
                {session
                  ? `${runtime ?? "Runtime"} / ${session.session_id.slice(0, 8)}`
                  : "尚无会话"}
              </span>
            ) : null}
            {session ? (
              <span className="eyebrow crumb-session-id mono">
                {runtime ?? "Runtime"} · {session.session_id.slice(0, 8)}
              </span>
            ) : (
              <span className="eyebrow">尚无会话</span>
            )}
            {issue.role ? (
              <span className="meta-chip">{roleLabel(issue.role)}</span>
            ) : null}
            {issue.priority ? (
              <span className="meta-chip mono">
                {String(issue.priority).toUpperCase()}
              </span>
            ) : null}
            {issue.stale === true ? (
              <span
                className="meta-chip chip-warning"
                title="目录数据可能滞后于 Gitea"
              >
                同步滞后
              </span>
            ) : null}
          </div>

          <div className="session-header-actions">
            {session ? (
              <span
                className={`connection-pill ${session.connection === "online" ? "online" : "offline"}`}
              >
                <span className="connection-dot" aria-hidden="true" />
                {session.connection === "online" ? "设备在线" : "设备未连接"}
              </span>
            ) : null}
            {giteaUrl ? (
              <a
                href={giteaUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="header-ext-link"
              >
                <span>Gitea</span>
                <ExternalLinkIcon size={12} />
              </a>
            ) : null}
          </div>
        </div>

        <div className="session-title-row">
          <h1>{String(issue.title ?? "选择一个 Issue")}</h1>
        </div>

        <div className="session-status-bar">
          <div className="session-status-left">
            <span className={`state-pill state-${tone}`}>
              <span className={`status-dot dot-${tone}`} aria-hidden="true" />
              {session
                ? (stateNames[session.state] ?? session.state)
                : closedIssue
                  ? "Issue 已关闭"
                  : "等待 Agent 认领并启动 Work"}
            </span>
            {sessions.length > 0 && onSelectSession ? (
              <label className="session-switcher">
                <span className="switcher-label">会话</span>
                <select
                  aria-label="切换会话"
                  value={activeSessionValue}
                  onChange={(e) => onSelectSession(e.target.value)}
                >
                  {sessions.map((s, idx) => (
                    <option key={s.session_id} value={s.session_id}>
                      #{idx + 1} ·{" "}
                      {runtimeLabel(s.runtime) ?? s.runtime ?? "Agent"} ·{" "}
                      {s.session_id.slice(0, 8)} ·{" "}
                      {stateNames[s.state] ?? s.state}
                    </option>
                  ))}
                </select>
                {multiActiveCount > 1 ? (
                  <span
                    className="multi-session-tip"
                    title="存在多个活跃会话，控制指令仅发送给当前选中的会话"
                  >
                    多会话目标
                  </span>
                ) : null}
              </label>
            ) : null}
          </div>

          <nav className="tabs" role="tablist" aria-label="视图切换">
            {(["记录", "原始事件", "详情"] as const).map((name) => (
              <button
                key={name}
                type="button"
                role="tab"
                aria-selected={tab === name}
                className={tab === name ? "selected" : ""}
                onClick={() => setTab(name)}
              >
                {name === "记录" ? "工作记录" : name}
                {name === "原始事件" && events.length > 0 ? (
                  <span className="tab-count mono">{events.length}</span>
                ) : null}
              </button>
            ))}
          </nav>
        </div>
      </header>

      <div
        className="timeline"
        ref={timeline}
        onScroll={(e) => {
          const node = e.currentTarget;
          following.current =
            node.scrollHeight - node.scrollTop - node.clientHeight < 60;
          if (following.current) setUnseen(0);
        }}
        aria-live="polite"
      >
        {tab === "记录" ? (
          <div className="conversation">
            {hasEarlier ? (
              <div className="load-earlier-wrap">
                <button
                  type="button"
                  className="btn-ghost"
                  onClick={() => onLoadEarlier?.()}
                >
                  加载更早记录
                </button>
              </div>
            ) : null}
            {loading ? (
              <div className="empty-state">
                <SpinnerIcon size={16} />
                <span>正在读取记录…</span>
              </div>
            ) : null}
            {!loading && records.length === 0 ? (
              <div className="empty-state-card">
                <p className="empty-title">
                  {closedIssue ? "此 Issue 没有会话记录" : "还没有工作记录"}
                </p>
                <p className="muted">
                  {closedIssue
                    ? "该任务已关闭，可在下方查看详情或补充历史评论。"
                    : session
                      ? "会话启动后的 Agent 消息、工具步骤和交付结果会实时呈现在这里。"
                      : "当符合角色的本地 Worker 认领该 Issue 后，工作记录将自动同步。"}
                </p>
              </div>
            ) : null}
            {records.length > 0 ? (
              <VirtualEvents
                ids={recordIds}
                scrollContainer={timeline}
                renderRow={(index) => (
                  <LogRow
                    entry={records[index]}
                    runtime={runtime}
                    currentUserName={currentUserName}
                  />
                )}
              />
            ) : null}

            {isWorking ? (
              <div className="working-indicator">
                <SpinnerIcon size={14} />
                <span>
                  {runtime ?? "Agent"}{" "}
                  {action ? `正在执行 · ${action}` : "正在工作…"}
                </span>
              </div>
            ) : null}

            {commands.length > 0 ? (
              <details className="receipts">
                <summary>
                  <RadioIcon size={13} />
                  <span>远程指令与回执</span>
                  <span className="pill-count">{commands.length}</span>
                </summary>
                <div className="receipts-list">
                  {commands.map((command, index) => {
                    const state = String(command.state ?? "unknown");
                    const isFail =
                      state === "failed" ||
                      state === "rejected" ||
                      state === "expired";
                    const isOk =
                      state === "applied" ||
                      state === "handled" ||
                      state === "accepted";
                    return (
                      <div
                        className="receipt-row"
                        key={String(command.command_id ?? index)}
                      >
                        <span className="receipt-type mono">
                          {String(command.type ?? "")}
                        </span>
                        <span
                          className={`badge ${isFail ? "badge-blocked" : isOk ? "badge-running" : "badge-idle"}`}
                        >
                          {commandStateNames[state] ?? state}
                        </span>
                        <span className="receipt-msg">
                          {String(command.message ?? "")}
                        </span>
                      </div>
                    );
                  })}
                </div>
              </details>
            ) : null}
          </div>
        ) : null}

        {tab === "原始事件" ? (
          <div className="raw-events-view">
            <div className="raw-toolbar">
              <div className="filter-pills">
                {[
                  ["all", "全部"],
                  ["message", "消息"],
                  ["tool", "工具/文件"],
                  ["state", "状态/交付"],
                  ["approval", "审批"],
                ].map(([key, label]) => (
                  <button
                    key={key}
                    type="button"
                    className={`filter-pill ${eventFilter === key ? "active" : ""}`}
                    onClick={() => setEventFilter(key)}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <div className="raw-search-box">
                <SearchIcon size={13} />
                <input
                  type="search"
                  placeholder="过滤事件类型或载荷…"
                  value={eventQuery}
                  onChange={(e) => setEventQuery(e.target.value)}
                />
              </div>
              <CopyButton
                value={JSON.stringify(filteredEvents, null, 2)}
                label="复制全部 JSON"
              />
            </div>

            {filteredEvents.length === 0 ? (
              <p className="empty">没有匹配的原始事件。</p>
            ) : (
              <div className="raw-event-list">
                {filteredEvents.map((event) => (
                  <details key={event.event_id} className="raw-event-row">
                    <summary>
                      <span className="raw-seq mono">#{event.sequence}</span>
                      <span className="raw-type mono">
                        {String(event.type ?? "unknown")}
                      </span>
                      <span className="raw-preview">{eventPreview(event)}</span>
                      <LogTimestamp
                        value={
                          typeof event.occurred_at === "string"
                            ? event.occurred_at
                            : undefined
                        }
                      />
                    </summary>
                    <div className="raw-event-body">
                      <div className="raw-event-actions">
                        <span className="mono muted">
                          event_id: {event.event_id}
                        </span>
                        <CopyButton
                          value={JSON.stringify(event, null, 2)}
                          label="复制事件"
                        />
                      </div>
                      <pre className="raw-json">
                        {JSON.stringify(event, null, 2)}
                      </pre>
                    </div>
                  </details>
                ))}
              </div>
            )}
          </div>
        ) : null}

        {tab === "详情" ? (
          <DetailsView
            issue={issue}
            session={session}
            projectName={projectName}
            giteaUrl={giteaUrl}
          />
        ) : null}
      </div>

      {unseen > 0 ? (
        <button type="button" className="new-events" onClick={follow}>
          {unseen} 条新记录 · 继续跟随 ↓
        </button>
      ) : null}

      <footer className="composer-shell">
        <div className="composer">
          {session?.state === "waiting_approval" ||
          session?.state === "waiting_input" ? (
            <div className="composer-notice notice-warning">
              <div className="composer-notice-text">
                <ShieldAlertIcon size={15} />
                <span>
                  请在 Agent 所在本机处理
                  {session.state === "waiting_input" ? "输入" : "审批"}
                  （远程不提供批准按钮）：
                </span>
                <code className="mono">
                  {session.state === "waiting_input"
                    ? "devmesh inputs list"
                    : "devmesh approvals list"}
                </code>
              </div>
              <CopyButton
                value={
                  session.state === "waiting_input"
                    ? "devmesh inputs list"
                    : "devmesh approvals list"
                }
                label="复制命令"
              />
            </div>
          ) : null}

          <textarea
            aria-label="消息"
            placeholder={
              allowed
                ? mode === "comment"
                  ? "添加 Issue 评论（留存记录，不直接干预运行中的 Agent）…"
                  : mode === "steer"
                    ? "向当前 Agent 发送任务调整指令 (Steer)…"
                    : mode === "follow_up"
                      ? "安排下一轮追加指令 (Follow up)…"
                      : "唤醒空闲会话并发送 Prompt…"
                : (disabledReason ?? "当前状态暂不支持此操作")
            }
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (
                (e.metaKey || e.ctrlKey) &&
                e.key === "Enter" &&
                allowed &&
                !sending &&
                text.trim()
              ) {
                e.preventDefault();
                void send();
              }
            }}
            disabled={!allowed || sending}
          />

          <div className="composer-footer">
            <div className="composer-mode-group">
              <RadioIcon size={14} className="composer-mode-icon" />
              <select
                aria-label="消息类型"
                value={mode}
                onChange={(e) => {
                  setMode(e.target.value);
                  setError("");
                }}
              >
                <option value="steer">纠正方向 · 当前任务</option>
                <option value="follow_up">Follow up · 下一轮</option>
                <option value="prompt">Prompt · 空闲会话</option>
                <option value="comment">评论 · 留存记录</option>
                <option value="interrupt">中断</option>
              </select>
              {disabledReason ? (
                <span className="composer-disabled-hint">{disabledReason}</span>
              ) : (
                <span className="composer-shortcut">⌘/Ctrl + Enter</span>
              )}
            </div>

            <div className="composer-actions">
              {error ? (
                <span role="alert" className="composer-error">
                  {error}
                </span>
              ) : null}
              {canInterruptNow && mode !== "interrupt" ? (
                confirmInterrupt ? (
                  <div className="interrupt-confirm">
                    <button
                      type="button"
                      className="btn-danger"
                      disabled={sending}
                      onClick={() => void send("interrupt", "")}
                    >
                      确认中断
                    </button>
                    <button
                      type="button"
                      className="btn-ghost-sm"
                      onClick={() => setConfirmInterrupt(false)}
                    >
                      取消
                    </button>
                  </div>
                ) : (
                  <button
                    type="button"
                    className="btn-danger-ghost"
                    disabled={sending}
                    onClick={() => setConfirmInterrupt(true)}
                  >
                    <StopCircleIcon size={14} />
                    <span>中断</span>
                  </button>
                )
              ) : null}
              <button
                type="button"
                className="send-btn primary"
                onClick={() => void send()}
                disabled={
                  !allowed || sending || (!text.trim() && mode !== "interrupt")
                }
                aria-label={sending ? "提交中" : "发送"}
              >
                {sending ? (
                  <SpinnerIcon size={14} />
                ) : (
                  <>
                    <span>{mode === "interrupt" ? "中断" : "发送"}</span>
                    <ArrowUpIcon size={14} />
                  </>
                )}
              </button>
            </div>
          </div>
        </div>
      </footer>
    </section>
  );
}

function eventPreview(event: WorkEvent): string {
  const payload = event.payload ?? {};
  if (typeof payload.text === "string") return payload.text.slice(0, 80);
  if (typeof payload.tool === "string")
    return `${payload.tool}${payload.outcome ? ` (${payload.outcome})` : ""}`;
  if (typeof payload.path === "string") return payload.path;
  if (typeof payload.state === "string")
    return `${payload.state}${payload.reason ? ` · ${payload.reason}` : ""}`;
  return "";
}

function DetailsView({
  issue,
  session,
  projectName,
  giteaUrl,
}: {
  issue: RecordData;
  session: Session | null;
  projectName?: string;
  giteaUrl: string | null;
}) {
  const num = issueNumber(issue);
  const assignees = Array.isArray(issue.assignees)
    ? (issue.assignees as unknown[]).map((a) =>
        typeof a === "string"
          ? a
          : String(
              (a as RecordData)?.login ?? (a as RecordData)?.username ?? a,
            ),
      )
    : [];
  const capabilities = Object.entries(session?.capabilities ?? {});
  const remoteControl = Object.entries(session?.remote_control ?? {});
  const permissions = Object.entries(session?.permission_flags ?? {});

  return (
    <div className="details-view">
      <div className="details-grid">
        <section className="detail-card">
          <h3>Issue 信息</h3>
          <dl className="detail-dl">
            <div>
              <dt>所属项目</dt>
              <dd>{projectName ?? String(issue.project_id ?? "—")}</dd>
            </div>
            <div>
              <dt>编号</dt>
              <dd className="mono">{num !== undefined ? `#${num}` : "—"}</dd>
            </div>
            <div>
              <dt>规范状态</dt>
              <dd>
                {String(issue.canonical_state ?? issue.source_state ?? "—")}
              </dd>
            </div>
            <div>
              <dt>执行角色</dt>
              <dd>{roleLabel(issue.role)}</dd>
            </div>
            <div>
              <dt>人工指派 (Assignees)</dt>
              <dd>
                {assignees.length > 0 ? assignees.join(", ") : "无人工预分配"}
              </dd>
            </div>
            <div>
              <dt>最近同步</dt>
              <dd className="mono">
                {typeof issue.synced_at === "string"
                  ? new Date(issue.synced_at).toLocaleString()
                  : "—"}
              </dd>
            </div>
            {giteaUrl ? (
              <div>
                <dt>Gitea 源链接</dt>
                <dd>
                  <a
                    href={giteaUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="md-link"
                  >
                    打开 Gitea Issue ↗
                  </a>
                </dd>
              </div>
            ) : null}
          </dl>
        </section>

        <section className="detail-card">
          <h3>当前 Work 与运行会话</h3>
          {session ? (
            <dl className="detail-dl">
              <div>
                <dt>Session ID</dt>
                <dd className="mono">{session.session_id}</dd>
              </div>
              <div>
                <dt>Work ID / 状态</dt>
                <dd className="mono">
                  {session.work_id.slice(0, 12)} · {session.work.state}
                </dd>
              </div>
              <div>
                <dt>Runtime</dt>
                <dd>
                  {runtimeLabel(session.runtime) ?? session.runtime ?? "—"}
                  {session.runtime_version
                    ? ` (${session.runtime_version})`
                    : ""}
                </dd>
              </div>
              <div>
                <dt>执行设备</dt>
                <dd className="mono">
                  {session.device_id ? session.device_id.slice(0, 12) : "—"} ·{" "}
                  {session.connection === "online" ? "在线" : "离线"}
                </dd>
              </div>
              <div>
                <dt>Claim 世代 / Epoch</dt>
                <dd className="mono">
                  gen {session.work.claim_generation ?? "—"} / epoch{" "}
                  {session.run_epoch}
                </dd>
              </div>
            </dl>
          ) : (
            <p className="muted">当前 Issue 尚未关联活跃或历史会话。</p>
          )}
        </section>
      </div>

      {session ? (
        <section className="detail-card">
          <h3>能力矩阵与远程控制策略</h3>
          <div className="matrix-sections">
            <div>
              <h4>账号权限</h4>
              <div className="flag-list">
                {permissions.map(([key, enabled]) => (
                  <span
                    key={key}
                    className={`flag-pill ${enabled ? "on" : "off"}`}
                  >
                    {key}
                  </span>
                ))}
              </div>
            </div>
            <div>
              <h4>本机远程控制策略</h4>
              <div className="flag-list">
                {remoteControl.map(([key, enabled]) => (
                  <span
                    key={key}
                    className={`flag-pill ${enabled ? "on" : "off"}`}
                  >
                    {key}
                  </span>
                ))}
              </div>
            </div>
            <div>
              <h4>Runtime 原生能力</h4>
              <div className="flag-list">
                {capabilities.map(([key, enabled]) => (
                  <span
                    key={key}
                    className={`flag-pill ${enabled ? "on" : "off"}`}
                  >
                    {key}
                  </span>
                ))}
              </div>
            </div>
          </div>
        </section>
      ) : null}

      <details className="detail-raw">
        <summary>查看原始 JSON 数据</summary>
        <div className="raw-event-body">
          <div className="raw-event-actions">
            <span className="muted">Issue & Session 完整快照</span>
            <CopyButton
              value={JSON.stringify({ issue, session }, null, 2)}
              label="复制 JSON"
            />
          </div>
          <pre className="raw-json">
            {JSON.stringify({ issue, session }, null, 2)}
          </pre>
        </div>
      </details>
    </div>
  );
}

const logTime = new Intl.DateTimeFormat("zh-CN", {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
});

function LogTimestamp({ value }: { value?: string }) {
  const timestamp = value ? Date.parse(value) : NaN;
  if (Number.isNaN(timestamp)) return null;
  return (
    <time dateTime={value} title={new Date(timestamp).toLocaleString()}>
      {logTime.format(timestamp)}
    </time>
  );
}

function PolicyBadges({ policy }: { policy?: ContentPolicy }) {
  if (!policy) return null;
  return (
    <div className="policy-badges">
      {policy.redacted ? <span className="policy-badge">已脱敏</span> : null}
      {policy.truncated ? (
        <span className="policy-badge">内容已截断</span>
      ) : null}
      {policy.omitted ? (
        <span className="policy-badge">部分片段已省略</span>
      ) : null}
    </div>
  );
}

function userAvatarText(name?: string): string {
  const trimmed = name?.trim();
  if (!trimmed) return "人";
  const chars = [...trimmed];
  if (/^[A-Za-z0-9]/.test(chars[0])) {
    return chars.slice(0, 2).join("").toUpperCase();
  }
  return chars[0];
}

function MessageRow({
  entry,
  runtime,
  currentUserName,
}: {
  entry: LogEntry;
  runtime?: string;
  currentUserName?: string;
}) {
  const isAgent = entry.kind === "agent";
  const userName = isAgent ? undefined : (entry.author ?? currentUserName);
  const displayName = isAgent
    ? (runtime ?? entry.title)
    : (userName ?? entry.title);
  return (
    <article className={`conversation-message conversation-${entry.kind}`}>
      <span
        className="conversation-avatar"
        aria-hidden="true"
        title={displayName}
      >
        {isAgent ? "AI" : userAvatarText(userName)}
      </span>
      <div className="conversation-body">
        <header className="conversation-meta">
          <strong>{displayName}</strong>
          {entry.source ? (
            <span className="msg-source-tag">{entry.source}</span>
          ) : null}
          <LogTimestamp value={entry.time} />
        </header>
        <div className="conversation-bubble">
          {isAgent && entry.text ? (
            <Markdown text={entry.text} />
          ) : (
            <p className="conversation-plain">{entry.text}</p>
          )}
          <PolicyBadges policy={entry.policy} />
        </div>
      </div>
    </article>
  );
}

function summarizeSteps(steps: LogEntry[]): string {
  const files = steps.filter((s) => s.toolCategory === "file").length;
  const cmds = steps.filter(
    (s) => s.toolCategory === "command" || s.toolCategory === "test",
  ).length;
  const parts: string[] = [];
  if (files > 0) parts.push(`文件操作 ${files}`);
  if (cmds > 0) parts.push(`命令/测试 ${cmds}`);
  return parts.length > 0 ? parts.join(" · ") : `${steps.length} 项步骤`;
}

function StepIcon({ step }: { step: LogEntry }) {
  if (step.running) return <SpinnerIcon size={13} className="step-icon-run" />;
  if (step.interrupted)
    return <StopCircleIcon size={13} className="step-icon-muted" />;
  if (step.kind === "warning" || step.outcome === "failed")
    return <AlertTriangleIcon size={13} className="step-icon-warn" />;
  if (step.toolCategory === "file")
    return <FileCodeIcon size={13} className="step-icon-file" />;
  if (step.toolCategory === "command" || step.toolCategory === "test")
    return <TerminalIcon size={13} className="step-icon-cmd" />;
  if (step.toolCategory === "delivery")
    return <GitCommitIcon size={13} className="step-icon-ok" />;
  return <CheckCircleIcon size={13} className="step-icon-ok" />;
}

function LogRow({
  entry,
  runtime,
  currentUserName,
}: {
  entry: LogEntry;
  runtime?: string;
  currentUserName?: string;
}) {
  if (entry.kind === "agent" || entry.kind === "user")
    return (
      <MessageRow
        entry={entry}
        runtime={runtime}
        currentUserName={currentUserName}
      />
    );

  if (entry.steps) {
    const running = entry.steps.some((step) => step.running);
    const hasWarning = entry.steps.some(
      (step) => step.kind === "warning" || step.outcome === "failed",
    );
    return (
      <details
        className={`conversation-activity ${running ? "is-running" : ""} ${hasWarning ? "has-warning" : ""}`}
        open={running || hasWarning}
      >
        <summary>
          <span className="activity-icon">
            {running ? (
              <SpinnerIcon size={14} />
            ) : hasWarning ? (
              <AlertTriangleIcon size={14} />
            ) : (
              <CheckCircleIcon size={14} />
            )}
          </span>
          <span className="activity-title">
            {entry.title} · {entry.steps.length} 项
          </span>
          <span className="activity-sub">{summarizeSteps(entry.steps)}</span>
          <LogTimestamp value={entry.time} />
          <span className="activity-chevron" aria-hidden="true">
            <ChevronRightIcon size={14} className="chev-closed" />
            <ChevronDownIcon size={14} className="chev-open" />
          </span>
        </summary>
        <div className="conversation-steps">
          {entry.steps.map((step) => (
            <LogRow key={step.id} entry={step} runtime={runtime} />
          ))}
        </div>
      </details>
    );
  }

  if (entry.approvalKind) {
    const cliCmd =
      entry.approvalKind === "input"
        ? "devmesh inputs list"
        : "devmesh approvals list";
    if (!entry.approvalActive) {
      return (
        <article className="log-entry log-approval-past">
          <header className="log-heading">
            <ShieldAlertIcon size={13} className="step-icon-muted" />
            <strong>{entry.title}</strong>
            <span className="log-inline-meta">历史请求</span>
            <LogTimestamp value={entry.time} />
          </header>
        </article>
      );
    }
    return (
      <article className="log-banner log-banner-warning">
        <ShieldAlertIcon size={15} className="banner-icon" />
        <div className="banner-body">
          <div className="banner-head">
            <strong>{entry.title}</strong>
            <LogTimestamp value={entry.time} />
          </div>
          <p className="banner-desc">
            请在执行设备终端运行 <code className="mono">{cliCmd}</code>{" "}
            完成确认。
          </p>
        </div>
        <CopyButton value={cliCmd} label="复制命令" />
      </article>
    );
  }

  if (entry.commitSha) {
    return (
      <article className="log-banner log-banner-delivery">
        <GitCommitIcon size={15} className="banner-icon" />
        <div className="banner-body">
          <div className="banner-head">
            <strong>{entry.title}</strong>
            <LogTimestamp value={entry.time} />
          </div>
          <p className="banner-desc mono">{entry.text}</p>
        </div>
        <CopyButton value={entry.commitSha} label="复制 SHA" />
      </article>
    );
  }

  return (
    <article className={`log-entry log-${entry.kind}`}>
      <header className="log-heading">
        <StepIcon step={entry} />
        <strong>{entry.title}</strong>
        {entry.text && entry.kind === "tool" ? (
          <span className="log-inline-meta mono">{entry.text}</span>
        ) : null}
        <LogTimestamp value={entry.time} />
      </header>
      {entry.text && entry.kind !== "tool" ? (
        <p className="log-text">{entry.text}</p>
      ) : null}
      {entry.detail ? <p className="log-text mono">{entry.detail}</p> : null}
      <PolicyBadges policy={entry.policy} />
      {entry.output ? (
        <details className="log-output">
          <summary>查看输出摘要</summary>
          <pre className="raw-json">{stripAnsi(entry.output)}</pre>
        </details>
      ) : null}
    </article>
  );
}
