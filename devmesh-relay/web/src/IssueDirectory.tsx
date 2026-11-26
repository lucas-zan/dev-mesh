import { useState } from "react";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  FolderIcon,
  StarIcon,
} from "./Icons.tsx";
import {
  isClosedIssue,
  issueNumber,
  issueStatus,
  roleLabel,
  type RecordData,
} from "./workspace.ts";

export interface Item extends RecordData {
  id: string;
  title?: string;
  name?: string;
}

export interface ProjectIssues {
  project: Item;
  issues: Item[];
}

export function IssueDirectory({
  projects,
  search,
  selectedId,
  showClosed = false,
  onlyActive = false,
  roleFilter = "",
  starredIds,
  onToggleStar,
  onSelect,
}: {
  projects: ProjectIssues[];
  search: string;
  selectedId?: string;
  showClosed?: boolean;
  onlyActive?: boolean;
  roleFilter?: string;
  starredIds?: ReadonlySet<string>;
  onToggleStar?: (issueId: string) => void;
  onSelect: (issue: Item) => void;
}) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const query = search.trim().toLowerCase();
  const normalizedRole = roleFilter.trim().toLowerCase();

  const groups = projects
    .map(({ project, issues }) => {
      const filtered = issues
        .filter((issue) => {
          if (!showClosed && isClosedIssue(issue)) return false;
          const status = issueStatus(issue);
          if (
            onlyActive &&
            status.tone !== "running" &&
            status.tone !== "waiting"
          ) {
            return false;
          }
          if (
            normalizedRole &&
            String(issue.role ?? "").toLowerCase() !== normalizedRole
          ) {
            return false;
          }
          if (!query) return true;
          const num = issueNumber(issue);
          const title = String(issue.title ?? "").toLowerCase();
          const numText = num !== undefined ? `#${num}` : "";
          const role = String(issue.role ?? "").toLowerCase();
          return (
            title.includes(query) ||
            numText.includes(query) ||
            role.includes(query)
          );
        })
        .sort((a, b) => {
          const sa = issueStatus(a);
          const sb = issueStatus(b);
          if (sa.priority !== sb.priority) return sa.priority - sb.priority;
          return (issueNumber(b) ?? 0) - (issueNumber(a) ?? 0);
        });
      const activeCount = filtered.filter((issue) => {
        const tone = issueStatus(issue).tone;
        return tone === "running" || tone === "waiting";
      }).length;
      return { project, issues: filtered, activeCount };
    })
    .filter(({ issues }) => issues.length > 0);

  return (
    <div className="issue-list" aria-label="项目和 Issue">
      {groups.map(({ project, issues, activeCount }, index) => {
        const open =
          Boolean(query) ||
          onlyActive ||
          (expanded[project.id] ??
            (index === 0 ||
              activeCount > 0 ||
              issues.some((issue) => issue.id === selectedId)));
        return (
          <section className="issue-group" key={project.id}>
            <button
              type="button"
              className="project-toggle"
              aria-expanded={open}
              disabled={Boolean(query)}
              onClick={() =>
                setExpanded((previous) => ({
                  ...previous,
                  [project.id]: !open,
                }))
              }
            >
              <span className="project-chevron" aria-hidden="true">
                {open ? (
                  <ChevronDownIcon size={13} />
                ) : (
                  <ChevronRightIcon size={13} />
                )}
              </span>
              <FolderIcon size={13} className="project-folder-icon" />
              <span className="project-name">{project.name ?? project.id}</span>
              {project.stale === true ? (
                <span
                  className="project-stale"
                  title="目录同步可能已过期"
                >
                  待同步
                </span>
              ) : null}
              <span className="project-count">
                {activeCount > 0 ? (
                  <span className="project-active-count">{activeCount}/</span>
                ) : null}
                {issues.length}
              </span>
            </button>
            {open && (
              <div className="project-issues">
                {issues.map((issue) => {
                  const status = issueStatus(issue);
                  const num = issueNumber(issue);
                  const starred = starredIds?.has(issue.id) ?? false;
                  const metaParts = [
                    num !== undefined ? `#${num}` : null,
                    status.runtime ?? null,
                    roleLabel(issue.role),
                  ].filter(Boolean);
                  return (
                    <div
                      key={issue.id}
                      className={`issue-item-wrap ${selectedId === issue.id ? "selected" : ""}`}
                    >
                      <button
                        type="button"
                        aria-current={
                          selectedId === issue.id ? true : undefined
                        }
                        className={`issue-item ${selectedId === issue.id ? "selected" : ""}`}
                        onClick={() => onSelect(issue)}
                      >
                        <span
                          className={`status-dot dot-${status.tone}`}
                          title={status.label}
                          aria-hidden="true"
                        />
                        <span className="issue-content">
                          <span className="issue-title">{issue.title}</span>
                          <small className="issue-meta">
                            {metaParts.map((part, idx) => (
                              <span
                                key={`${part}-${idx}`}
                                className={
                                  part?.startsWith("#") ? "mono" : undefined
                                }
                              >
                                {idx > 0 ? " · " : ""}
                                {part}
                              </span>
                            ))}
                            {status.activeCount > 1 ? (
                              <span className="issue-multi-badge">
                                {" "}
                                ×{status.activeCount}
                              </span>
                            ) : null}
                          </small>
                        </span>
                      </button>
                      {onToggleStar ? (
                        <button
                          type="button"
                          className={`issue-star ${starred ? "starred" : ""}`}
                          aria-label={starred ? "取消关注" : "关注 Issue"}
                          aria-pressed={starred}
                          onClick={(e) => {
                            e.stopPropagation();
                            onToggleStar(issue.id);
                          }}
                        >
                          <StarIcon size={12} filled={starred} />
                        </button>
                      ) : null}
                    </div>
                  );
                })}
                {issues.length === 0 && (
                  <p className="empty">暂无同步的 Issue</p>
                )}
              </div>
            )}
          </section>
        );
      })}
      {groups.length === 0 && (
        <p className="empty">
          {query || onlyActive || normalizedRole
            ? "没有匹配的 Issue"
            : "暂无会话记录"}
        </p>
      )}
    </div>
  );
}
