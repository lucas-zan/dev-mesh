import {
  ActivityIcon,
  AlertTriangleIcon,
  CheckCircleIcon,
  MonitorIcon,
  StarIcon,
} from "./Icons.tsx";
import type { Item, ProjectIssues } from "./IssueDirectory.tsx";
import {
  isClosedIssue,
  issueNumber,
  issueStatus,
  roleLabel,
  type RecordData,
} from "./workspace.ts";

interface OverviewProps {
  directory: ProjectIssues[];
  devices: RecordData[];
  starredIds: ReadonlySet<string>;
  onSelectIssue: (issue: Item) => void;
  onOpenAdmin?: () => void;
}

export function Overview({
  directory,
  devices,
  starredIds,
  onSelectIssue,
  onOpenAdmin,
}: OverviewProps) {
  const flat = directory.flatMap(({ project, issues }) =>
    issues.map((issue) => ({
      project,
      issue,
      status: issueStatus(issue),
    })),
  );
  const open = flat.filter(({ issue }) => !isClosedIssue(issue));
  const running = open.filter(({ status }) => status.tone === "running");
  const attention = open.filter(
    ({ status }) => status.tone === "waiting" || status.tone === "blocked",
  );
  const review = open.filter(({ status }) => status.tone === "review");
  const onlineDevices = devices.filter((device) => device.online === true);
  const starred = flat.filter(({ issue }) => starredIds.has(issue.id));

  const priorityList = [...attention, ...running, ...review].slice(0, 10);

  return (
    <section className="overview">
      <header className="overview-header">
        <div>
          <div className="eyebrow">DEVMESH WORKSPACE</div>
          <h1>工作台概览</h1>
          <p className="muted">
            实时追踪团队项目中的 Agent 执行状态、待审批任务与本机设备连接。
          </p>
        </div>
        {onOpenAdmin ? (
          <button
            type="button"
            className="btn-ghost"
            onClick={onOpenAdmin}
          >
            工作区设置
          </button>
        ) : null}
      </header>

      <div className="kpi-grid">
        <div className="kpi-card kpi-running">
          <div className="kpi-label">
            <ActivityIcon size={14} />
            <span>运行中任务</span>
          </div>
          <div className="kpi-value">{running.length}</div>
          <div className="kpi-sub">在线实时执行会话</div>
        </div>
        <div className="kpi-card kpi-waiting">
          <div className="kpi-label">
            <AlertTriangleIcon size={14} />
            <span>需关注处理</span>
          </div>
          <div className="kpi-value">{attention.length}</div>
          <div className="kpi-sub">等待本机审批 / 受阻</div>
        </div>
        <div className="kpi-card kpi-review">
          <div className="kpi-label">
            <CheckCircleIcon size={14} />
            <span>待审核 Review</span>
          </div>
          <div className="kpi-value">{review.length}</div>
          <div className="kpi-sub">等待人工合并或验收</div>
        </div>
        <div className="kpi-card kpi-device">
          <div className="kpi-label">
            <MonitorIcon size={14} />
            <span>在线执行设备</span>
          </div>
          <div className="kpi-value">
            {onlineDevices.length}
            <span className="kpi-denom">/{devices.length}</span>
          </div>
          <div className="kpi-sub">审批均在执行本机完成</div>
        </div>
      </div>

      <div className="overview-columns">
        <section className="overview-card">
          <header className="overview-card-head">
            <h2>活跃与待处理任务</h2>
            <span className="pill-count">{priorityList.length}</span>
          </header>
          {priorityList.length === 0 ? (
            <div className="overview-empty">
              <p>当前没有运行中或待处理的 Issue。</p>
              <p className="muted">
                从左侧项目树选择任意任务查看历史工作记录与交付结果。
              </p>
            </div>
          ) : (
            <div className="overview-list">
              {priorityList.map(({ project, issue, status }) => {
                const num = issueNumber(issue);
                return (
                  <button
                    type="button"
                    key={issue.id}
                    className="overview-row"
                    onClick={() => onSelectIssue(issue)}
                  >
                    <span
                      className={`status-dot dot-${status.tone}`}
                      aria-hidden="true"
                    />
                    <div className="overview-row-main">
                      <span className="overview-row-title">
                        {issue.title ?? issue.id}
                      </span>
                      <span className="overview-row-meta">
                        <span>{project.name ?? project.id}</span>
                        {num !== undefined ? (
                          <span className="mono">#{num}</span>
                        ) : null}
                        {status.runtime ? <span>{status.runtime}</span> : null}
                        <span>{roleLabel(issue.role)}</span>
                      </span>
                    </div>
                    <span className={`badge badge-${status.tone}`}>
                      {status.label}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </section>

        <div className="overview-side">
          <section className="overview-card">
            <header className="overview-card-head">
              <h2>我的执行设备</h2>
              <span className="pill-count">{devices.length}</span>
            </header>
            {devices.length === 0 ? (
              <p className="overview-empty muted">暂无绑定的执行设备。</p>
            ) : (
              <div className="device-list">
                {devices.map((device) => {
                  const online = device.online === true;
                  const id = String(device.device_id ?? device.id ?? "");
                  return (
                    <div className="device-row" key={id}>
                      <span
                        className={`status-dot ${online ? "dot-running" : "dot-idle"}`}
                        aria-hidden="true"
                      />
                      <div className="device-row-info">
                        <strong>
                          {String(device.name ?? "未命名设备")}
                        </strong>
                        <code className="mono">{id.slice(0, 12)}</code>
                      </div>
                      <span
                        className={`badge ${online ? "badge-running" : "badge-idle"}`}
                      >
                        {online ? "在线" : "离线"}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </section>

          <section className="overview-card">
            <header className="overview-card-head">
              <h2>
                <StarIcon size={13} filled /> 关注的任务
              </h2>
              <span className="pill-count">{starred.length}</span>
            </header>
            {starred.length === 0 ? (
              <p className="overview-empty muted">
                在左侧点击任务星标即可固定关注。
              </p>
            ) : (
              <div className="overview-list">
                {starred.map(({ project, issue, status }) => (
                  <button
                    type="button"
                    key={issue.id}
                    className="overview-row"
                    onClick={() => onSelectIssue(issue)}
                  >
                    <span
                      className={`status-dot dot-${status.tone}`}
                      aria-hidden="true"
                    />
                    <div className="overview-row-main">
                      <span className="overview-row-title">
                        {issue.title ?? issue.id}
                      </span>
                      <span className="overview-row-meta">
                        <span>{project.name ?? project.id}</span>
                        <span>{status.label}</span>
                      </span>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </section>
        </div>
      </div>
    </section>
  );
}
