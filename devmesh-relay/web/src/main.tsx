import { useEffect, useState, useCallback, useRef, useMemo } from "react";
import { createRoot } from "react-dom/client";
import { Admin } from "./Admin.tsx";
import { subscribeFeed } from "./feed.ts";
import {
  ActivityIcon,
  LogOutIcon,
  LogoMark,
  MenuIcon,
  OverviewIcon,
  RefreshIcon,
  SearchIcon,
  SettingsIcon,
  XIcon,
} from "./Icons.tsx";
import {
  IssueDirectory,
  type Item,
  type ProjectIssues,
} from "./IssueDirectory.tsx";
import { Overview } from "./Overview.tsx";
import { SessionPanel } from "./SessionPanel.tsx";
import {
  api,
  buildCommand,
  isClosedIssue,
  issueNumber,
  issueStatus,
  mergeEvents,
  type Session,
  type WorkEvent,
  type RecordData,
} from "./workspace.ts";
import "./style.css";

interface Page<T> {
  items: T[];
  next_cursor?: string;
  as_of_cursor?: string;
}

async function all<T>(
  path: string,
  snapshot?: (cursor: string) => void,
): Promise<T[]> {
  const result: T[] = [];
  let cursor: string | undefined;
  do {
    const page = await api<Page<T>>(
      `${path}${path.includes("?") ? "&" : "?"}limit=200${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`,
    );
    if (page.as_of_cursor) snapshot?.(page.as_of_cursor);
    result.push(...page.items);
    cursor = page.next_cursor ?? undefined;
  } while (cursor);
  return result;
}

function App() {
  const [user, setUser] = useState<RecordData | null>(null);
  const [authChecked, setAuthChecked] = useState(false);
  const [loginError, setLoginError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    api<RecordData>("/me")
      .then(setUser)
      .catch(() => setUser(null))
      .finally(() => setAuthChecked(true));
  }, []);

  async function login(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const username = String(form.get("username") ?? "").trim();
    setSubmitting(true);
    setLoginError("");
    try {
      await api("/auth/login", {
        method: "POST",
        body: JSON.stringify(Object.fromEntries(form)),
      });
      if (username) sessionStorage.setItem("devmesh.username", username);
      setUser(await api("/me"));
    } catch (e) {
      setLoginError(String(e));
    } finally {
      setSubmitting(false);
    }
  }

  if (!authChecked) {
    return (
      <main className="login">
        <div className="login-loading">
          <LogoMark size={28} />
          <span>正在连接 DevMesh…</span>
        </div>
      </main>
    );
  }

  if (!user)
    return (
      <main className="login">
        <form onSubmit={login} className="login-card">
          <div className="brand-lockup">
            <LogoMark size={26} />
            <div>
              <div className="brand-title">DevMesh</div>
              <div className="brand-sub">RELAY WORKSPACE</div>
            </div>
          </div>
          <h1>连接你的开发工作</h1>
          <p className="login-desc">
            使用 Relay 账户登录，实时查看团队项目与 Agent 运行会话。
          </p>
          <label>
            用户名
            <input
              name="username"
              autoComplete="username"
              placeholder="输入用户名"
              required
            />
          </label>
          <label>
            密码
            <input
              name="password"
              type="password"
              autoComplete="current-password"
              placeholder="输入密码"
              required
            />
          </label>
          {loginError ? (
            <p role="alert" className="login-error">
              {loginError}
            </p>
          ) : null}
          <button className="primary login-submit" disabled={submitting}>
            {submitting ? "正在登录…" : "登录工作台"}
          </button>
        </form>
      </main>
    );

  return (
    <Workspace
      user={user}
      onLogout={async () => {
        await api("/auth/logout", { method: "POST" });
        sessionStorage.removeItem("devmesh.username");
        setUser(null);
      }}
    />
  );
}

function Workspace({
  user,
  onLogout,
}: {
  user: RecordData;
  onLogout: () => Promise<void>;
}) {
  const [directory, setDirectory] = useState<ProjectIssues[]>([]);
  const [devices, setDevices] = useState<RecordData[]>([]);
  const [starredIds, setStarredIds] = useState<Set<string>>(() => new Set());
  const [issue, setIssue] = useState<Item | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionId, setSessionId] = useState("");
  const [session, setSession] = useState<Session | null>(null);
  const selected = useRef({ sessionId: "", issueId: "" });
  selected.current = { sessionId, issueId: issue?.id ?? "" };

  const [hasEarlier, setHasEarlier] = useState(false);
  const historyInitialized = useRef("");
  const [feedCursor, setFeedCursor] = useState<string | null>(null);
  const [events, setEvents] = useState<WorkEvent[]>([]);
  const [commands, setCommands] = useState<RecordData[]>([]);
  const [comments, setComments] = useState<RecordData[]>([]);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const [search, setSearch] = useState("");
  const [showClosed, setShowClosed] = useState(false);
  const [includeUnclaimed, setIncludeUnclaimed] = useState(false);
  const [onlyActive, setOnlyActive] = useState(false);
  const [roleFilter, setRoleFilter] = useState("");
  const [admin, setAdmin] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const refreshDevices = useCallback(async () => {
    try {
      const page = await api<Page<RecordData>>("/devices");
      setDevices(page.items ?? []);
    } catch {
      // Non-blocking auxiliary data
    }
  }, []);

  const refreshDirectory = useCallback(
    async (rememberCursor = false) => {
      let baseline: string | undefined;
      const suffix = includeUnclaimed ? "" : "?with_sessions=true";
      const projects = await all<Item>(`/projects${suffix}`, (cursor) => {
        baseline ??= cursor;
      });
      const groups = await Promise.all(
        projects.map(async (project) => ({
          project,
          issues: await all<Item>(`/projects/${project.id}/issues${suffix}`),
        })),
      );
      setDirectory(groups);
      setIssue((current) =>
        current &&
        groups.some(({ issues }) =>
          issues.some((item) => item.id === current.id),
        )
          ? current
          : null,
      );
      if (rememberCursor) setFeedCursor(baseline ?? "0");
    },
    [includeUnclaimed],
  );

  useEffect(() => {
    refreshDirectory(feedCursor === null).catch((e) => setError(String(e)));
  }, [refreshDirectory]);

  useEffect(() => {
    void refreshDevices();
    api<{ items?: unknown[] }>("/subscriptions")
      .then((res) => {
        if (Array.isArray(res.items)) {
          setStarredIds(new Set(res.items.map(String)));
        }
      })
      .catch(() => {});
  }, [refreshDevices]);

  const toggleStar = useCallback(
    (issueId: string) => {
      setStarredIds((previous) => {
        const next = new Set(previous);
        if (next.has(issueId)) next.delete(issueId);
        else next.add(issueId);
        void api("/subscriptions", {
          method: "PUT",
          body: JSON.stringify({ items: [...next] }),
        }).catch(() => {});
        return next;
      });
    },
    [],
  );

  const refreshIssue = useCallback(async () => {
    if (!issue) return;
    const [detail, works] = await Promise.all([
      api<Item>(`/issues/${issue.id}`),
      all<RecordData>(`/issues/${issue.id}/works`),
    ]);
    if (selected.current.issueId !== issue.id) return;
    setIssue((previous) =>
      previous?.id === detail.id ? { ...previous, ...detail } : previous,
    );
    setComments((detail.comments as RecordData[] | undefined) ?? []);
    const found = works
      .sort((a, b) => String(a.started_at).localeCompare(String(b.started_at)))
      .flatMap((work) => (work.sessions as Session[] | undefined) ?? []);
    setSessions(found);
    setSessionId((previous) =>
      found.some((s) => s.session_id === previous)
        ? previous
        : (found.at(-1)?.session_id ?? ""),
    );
  }, [issue?.id]);

  useEffect(() => {
    setEvents([]);
    setComments([]);
    setCommands([]);
    setSession(null);
    refreshIssue().catch((e) => setError(String(e)));
  }, [refreshIssue]);

  const refreshSession = useCallback(async () => {
    if (!sessionId) {
      setSession(null);
      return;
    }
    const [current, newEvents, newCommands] = await Promise.all([
      api<Session>(`/sessions/${sessionId}`),
      api<Page<WorkEvent>>(
        `/sessions/${sessionId}/events?limit=100&order=desc`,
      ),
      all<RecordData>(`/sessions/${sessionId}/commands`),
    ]);
    if (selected.current.sessionId !== sessionId) return;
    setSession(current);
    setEvents((previous) => mergeEvents(previous, newEvents.items));
    if (historyInitialized.current !== sessionId) {
      setHasEarlier(Boolean(newEvents.next_cursor));
      historyInitialized.current = sessionId;
    }
    setCommands(newCommands);
  }, [sessionId]);

  useEffect(() => {
    setEvents([]);
    historyInitialized.current = "";
    setHasEarlier(false);
    setLoading(true);
    refreshSession()
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [refreshSession]);

  const refreshCurrent = useRef(async (_kinds: ReadonlySet<string>) => {});
  refreshCurrent.current = async (kinds) => {
    const refreshList = [
      "snapshot",
      "project.changed",
      "catalog.changed",
      "issue.changed",
      "session.changed",
      "work.changed",
    ].some((kind) => kinds.has(kind));
    const refreshDev = kinds.has("snapshot") || kinds.has("device.changed");
    await Promise.all([
      refreshList && refreshDirectory(),
      refreshDev && refreshDevices(),
      refreshIssue(),
      refreshSession(),
    ]);
    setError("");
  };

  useEffect(() => {
    if (feedCursor === null) return;
    const feed = subscribeFeed(
      feedCursor,
      (kinds) => refreshCurrent.current(kinds),
      (err) => setError(String(err)),
    );
    return () => feed.close();
  }, [feedCursor]);

  async function loadEarlier() {
    if (!sessionId || events.length === 0) return;
    const page = await api<Page<WorkEvent>>(
      `/sessions/${sessionId}/events?limit=100&order=desc&before_sequence=${events[0].sequence}`,
    );
    if (selected.current.sessionId !== sessionId) return;
    setEvents((previous) => mergeEvents(previous, page.items));
    setHasEarlier(Boolean(page.next_cursor));
  }

  async function send(mode: string, text: string) {
    if (!issue) throw new Error("请选择 Issue");
    if (mode === "comment")
      await api(`/issues/${issue.id}/comments`, {
        method: "POST",
        body: JSON.stringify({ message: text, work_id: session?.work_id }),
      });
    else {
      if (!session) throw new Error("请选择 Session");
      await api(`/sessions/${session.session_id}/commands`, {
        method: "POST",
        headers: { "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify(buildCommand(mode, text, session)),
      });
    }
    await Promise.all([refreshIssue(), refreshSession()]);
  }

  const activeIssueCount = useMemo(() => {
    let count = 0;
    for (const group of directory) {
      for (const item of group.issues) {
        if (isClosedIssue(item)) continue;
        const tone = issueStatus(item).tone;
        if (tone === "running" || tone === "waiting") count++;
      }
    }
    return count;
  }, [directory]);

  const currentProjectName = useMemo(() => {
    if (!issue) return undefined;
    return directory.find(({ issues }) =>
      issues.some((item) => item.id === issue.id),
    )?.project.name;
  }, [directory, issue]);

  function selectIssue(item: Item) {
    if (item.id !== issue?.id) {
      setSessions([]);
      setSessionId("");
      setSession(null);
      setIssue(item);
    }
    setAdmin(false);
    setDrawerOpen(false);
  }

  const currentUserName =
    (typeof user.display_name === "string" && user.display_name.trim()) ||
    (typeof user.username === "string" && user.username.trim()) ||
    sessionStorage.getItem("devmesh.username") ||
    undefined;

  return (
    <main className="workspace">
      {drawerOpen ? (
        <div
          className="sidebar-backdrop"
          onClick={() => setDrawerOpen(false)}
          aria-hidden="true"
        />
      ) : null}

      <aside className={`sidebar ${drawerOpen ? "open" : ""}`}>
        <div className="sidebar-brand">
          <div className="brand-lockup">
            <LogoMark size={20} />
            <span className="brand-name">DevMesh</span>
          </div>
          <div className="sidebar-brand-right">
            <span
              className={`live-indicator ${error ? "degraded" : "live"}`}
              title={error ? "实时连接异常" : "SSE 实时同步正常"}
            />
            <button
              type="button"
              className="icon-btn mobile-close"
              aria-label="关闭侧边栏"
              onClick={() => setDrawerOpen(false)}
            >
              <XIcon size={16} />
            </button>
          </div>
        </div>

        <nav className="sidebar-nav" aria-label="主导航">
          <button
            type="button"
            className={`nav-item ${!admin && !issue && !onlyActive ? "active" : ""}`}
            onClick={() => {
              setAdmin(false);
              setIssue(null);
              setOnlyActive(false);
              setDrawerOpen(false);
            }}
          >
            <OverviewIcon size={15} />
            <span>概览 Overview</span>
          </button>
          <button
            type="button"
            className={`nav-item ${onlyActive ? "active" : ""}`}
            onClick={() => {
              setOnlyActive((prev) => !prev);
              setAdmin(false);
            }}
          >
            <ActivityIcon size={15} className="nav-active-icon" />
            <span>活跃任务</span>
            <span className="nav-badge mono">{activeIssueCount}</span>
          </button>
        </nav>

        <div className="sidebar-section-head">
          <span className="sidebar-section-label">PROJECTS</span>
          <div className="sidebar-toggles">
            <button
              type="button"
              className={`mini-toggle ${includeUnclaimed ? "active" : ""}`}
              aria-pressed={includeUnclaimed}
              title="显示尚未启动会话的已同步 Issue"
              onClick={() => setIncludeUnclaimed((v) => !v)}
            >
              全部
            </button>
            <button
              type="button"
              className={`mini-toggle ${showClosed ? "active" : ""}`}
              aria-pressed={showClosed}
              onClick={() => setShowClosed((v) => !v)}
            >
              {showClosed ? "含已关闭" : "已关闭"}
            </button>
          </div>
        </div>

        <div className="sidebar-search-wrap">
          <SearchIcon size={14} className="sidebar-search-icon" />
          <input
            className="search"
            aria-label="搜索 Issue"
            placeholder="搜索标题或 #编号…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {search ? (
            <button
              type="button"
              className="search-clear"
              aria-label="清空搜索"
              onClick={() => setSearch("")}
            >
              <XIcon size={12} />
            </button>
          ) : null}
        </div>

        <div className="role-filter-row" aria-label="角色筛选">
          {[
            ["", "全部"],
            ["backend", "BE"],
            ["frontend", "FE"],
            ["qa", "QA"],
            ["product", "PM"],
          ].map(([key, label]) => (
            <button
              key={key}
              type="button"
              className={`role-chip ${roleFilter === key ? "active" : ""}`}
              onClick={() => setRoleFilter(key)}
            >
              {label}
            </button>
          ))}
        </div>

        <IssueDirectory
          showClosed={showClosed}
          onlyActive={onlyActive}
          roleFilter={roleFilter}
          projects={directory}
          search={search}
          selectedId={admin ? undefined : issue?.id}
          starredIds={starredIds}
          onToggleStar={toggleStar}
          onSelect={selectIssue}
        />

        <footer className="sidebar-footer">
          {user.admin ? (
            <button
              type="button"
              className={`footer-nav-btn ${admin ? "active" : ""}`}
              onClick={() => {
                setAdmin(true);
                setDrawerOpen(false);
              }}
            >
              <SettingsIcon size={14} />
              <span>设置</span>
            </button>
          ) : null}
          <button
            type="button"
            className="footer-nav-btn"
            onClick={() => onLogout().catch((e) => setError(String(e)))}
          >
            <LogOutIcon size={14} />
            <span>退出</span>
          </button>
        </footer>
      </aside>

      <div className="main-pane">
        <div className="mobile-topbar">
          <button
            type="button"
            className="icon-btn"
            aria-label="打开侧边栏"
            onClick={() => setDrawerOpen(true)}
          >
            <MenuIcon size={17} />
          </button>
          <span className="mobile-title">
            {admin
              ? "工作区设置"
              : issue
                ? String(issue.title ?? "Issue")
                : "DevMesh 工作台"}
          </span>
        </div>

        {error ? (
          <div role="alert" className="error-banner">
            <span>{error}</span>
            <button
              type="button"
              className="btn-ghost-sm"
              onClick={() => {
                if (feedCursor === null)
                  refreshDirectory(true).catch((e) => setError(String(e)));
                else
                  refreshCurrent
                    .current(new Set(["snapshot"]))
                    .catch((e) => setError(String(e)));
              }}
            >
              <RefreshIcon size={12} />
              <span>刷新</span>
            </button>
          </div>
        ) : null}

        {admin ? (
          <Admin
            onClose={() => setAdmin(false)}
            returnLabel={
              issue
                ? `返回 ${issueNumber(issue) !== undefined ? `#${issueNumber(issue)}` : "当前任务"}`
                : "返回工作台"
            }
          />
        ) : issue ? (
          <SessionPanel
            session={session}
            sessions={sessions}
            sessionId={sessionId}
            onSelectSession={setSessionId}
            projectName={currentProjectName}
            currentUserName={currentUserName}
            events={events}
            commands={commands}
            comments={comments}
            issue={issue}
            onSend={send}
            loading={loading}
            hasEarlier={hasEarlier}
            onLoadEarlier={() =>
              loadEarlier().catch((e) => setError(String(e)))
            }
          />
        ) : (
          <Overview
            directory={directory}
            devices={devices}
            starredIds={starredIds}
            onSelectIssue={selectIssue}
            onOpenAdmin={user.admin ? () => setAdmin(true) : undefined}
          />
        )}
      </div>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
