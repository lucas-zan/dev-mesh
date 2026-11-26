import { useCallback, useEffect, useState } from "react";
import {
  ArrowLeftIcon,
  CheckIcon,
  CopyIcon,
  PlusIcon,
  XIcon,
} from "./Icons.tsx";
import { api, type RecordData } from "./workspace.ts";
import {
  adminIdentifier,
  adminPayload,
  adminRecordAction,
} from "./admin-model.ts";

const scopes = [
  ["read", "查看"],
  ["comment", "评论"],
  ["steer", "控制会话"],
  ["interrupt", "中断"],
  ["ingest", "上传事件"],
  ["catalog:write", "同步 Issue"],
  ["manage", "管理项目"],
];

interface Page {
  items: RecordData[];
}

type ModalKind = "create" | "member" | null;

function CopyableCode({
  value,
  ariaLabel = "复制",
}: {
  value: string;
  ariaLabel?: string;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="device-identifier">
      <code className="mono">{value}</code>
      {typeof navigator !== "undefined" && navigator.clipboard ? (
        <button
          type="button"
          className="copy-inline-btn"
          aria-label={ariaLabel}
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(value);
              setCopied(true);
              setTimeout(() => setCopied(false), 1600);
            } catch {
              setCopied(false);
            }
          }}
        >
          {copied ? <CheckIcon size={12} /> : <CopyIcon size={12} />}
          <span>{copied ? "已复制" : "复制"}</span>
        </button>
      ) : null}
    </div>
  );
}

export function Admin({
  onClose,
  returnLabel = "返回工作台",
}: {
  onClose?: () => void;
  returnLabel?: string;
} = {}) {
  const [collection, setCollection] = useState("projects");
  const [records, setRecords] = useState<Record<string, RecordData[]>>({});
  const [users, setUsers] = useState<RecordData[]>([]);
  const [projects, setProjects] = useState<RecordData[]>([]);
  const [statusMessage, setStatusMessage] = useState("");
  const [modalError, setModalError] = useState("");
  const [secret, setSecret] = useState("");
  const [newDeviceId, setNewDeviceId] = useState("");
  const [busy, setBusy] = useState(false);
  const [modal, setModal] = useState<ModalKind>(null);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (modal) {
        setModal(null);
        setModalError("");
        return;
      }
      if (!onClose) return;
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [modal, onClose]);

  const items = records[collection] ?? [];
  const load = useCallback(async () => {
    const page = await api<Page>(`/admin/${collection}`);
    setRecords((current) => ({ ...current, [collection]: page.items }));
  }, [collection]);

  useEffect(() => {
    setSecret("");
    setNewDeviceId("");
    setStatusMessage("");
    setModal(null);
    setModalError("");
    load().catch((e) => setStatusMessage(String(e)));
  }, [load]);

  useEffect(() => {
    Promise.all([api<Page>("/admin/users"), api<Page>("/admin/projects")])
      .then(([u, p]) => {
        setUsers(u.items);
        setProjects(p.items);
      })
      .catch((e) => setStatusMessage(String(e)));
  }, []);

  async function create(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    setBusy(true);
    setModalError("");
    setStatusMessage("");
    try {
      const result = await api<RecordData>(`/admin/${collection}`, {
        method: "POST",
        body: JSON.stringify(adminPayload(collection, new FormData(form))),
      });
      setSecret(String(result.key ?? result.token ?? ""));
      setNewDeviceId(String(result.device_id ?? ""));
      form.reset();
      setModal(null);
      await load();
      if (collection === "projects")
        setProjects((await api<Page>("/admin/projects")).items);
      if (collection === "users")
        setUsers((await api<Page>("/admin/users")).items);
    } catch (e) {
      setModalError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove(path: string) {
    setStatusMessage("");
    try {
      await api(path, {
        method: "DELETE",
      });
      await load();
    } catch (e) {
      setStatusMessage(String(e));
    }
  }

  async function member(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setBusy(true);
    setModalError("");
    try {
      await api(`/admin/projects/${String(form.get("project_id"))}/members`, {
        method: "PUT",
        body: JSON.stringify({
          user_id: form.get("user_id"),
          permissions: form.getAll("permissions"),
        }),
      });
      setModal(null);
      setStatusMessage("项目权限已保存。");
    } catch (e) {
      setModalError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const createLabel =
    collection === "projects"
      ? "添加项目"
      : collection === "users"
        ? "添加账户"
        : collection === "keys"
          ? "创建设备密钥"
          : null;

  return (
    <section className="admin">
      <header className="admin-header">
        <div className="admin-header-row">
          <div>
            <div className="eyebrow">WORKSPACE SETTINGS</div>
            <h1>工作区设置</h1>
          </div>
          {onClose ? (
            <button
              type="button"
              className="btn-ghost admin-back-btn"
              onClick={onClose}
              title="按 Esc 也可返回"
            >
              <ArrowLeftIcon size={14} />
              <span>{returnLabel}</span>
            </button>
          ) : null}
        </div>
        <p className="muted">
          管理关联的 Gitea 仓库、团队账户权限、设备密钥与执行设备。
        </p>
      </header>

      <div className="admin-toolbar">
        <nav className="admin-tabs" role="tablist">
          {[
            ["projects", "项目"],
            ["users", "账户"],
            ["keys", "设备密钥"],
            ["devices", "设备"],
          ].map(([key, label]) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={collection === key}
              className={collection === key ? "selected" : ""}
              onClick={() => setCollection(key)}
            >
              {label}
            </button>
          ))}
        </nav>

        <div className="admin-toolbar-actions">
          {collection === "projects" ? (
            <button
              type="button"
              className="btn-ghost"
              disabled={projects.length === 0 || users.length === 0}
              onClick={() => {
                setModalError("");
                setModal("member");
              }}
            >
              成员权限
            </button>
          ) : null}
          {createLabel ? (
            <button
              type="button"
              className="primary admin-create-btn"
              onClick={() => {
                setModalError("");
                setModal("create");
              }}
            >
              <PlusIcon size={14} />
              <span>{createLabel}</span>
            </button>
          ) : null}
        </div>
      </div>

      {statusMessage ? (
        <p role="status" className="notice">
          {statusMessage}
        </p>
      ) : null}

      {secret ? (
        <aside className="secret">
          <div className="secret-head">
            <h2>请保存新密钥</h2>
            <CopyableCode value={secret} ariaLabel="复制新密钥" />
          </div>
          <p>密钥仅在本次创建时返回，请保存到本机凭证配置中。</p>
          <textarea aria-label="新密钥" readOnly value={secret} />
          {newDeviceId ? (
            <div className="secret-device">
              <p>
                设备 ID（填入本机配置的 <code className="mono">device_id</code>）
              </p>
              <CopyableCode value={newDeviceId} ariaLabel="复制设备 ID" />
            </div>
          ) : null}
          <button
            type="button"
            className="btn-ghost"
            onClick={() => setSecret("")}
          >
            已保存，关闭显示
          </button>
        </aside>
      ) : null}

      <div className="admin-card admin-records">
        {items.length === 0 ? (
          <p className="empty">暂无记录</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>名称</th>
                <th>标识</th>
                <th>状态 / 权限</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {items.map((item) => {
                const id = String(item.id);
                const action = adminRecordAction(collection, item);
                return (
                  <tr key={id}>
                    <td className="admin-name-cell">
                      {String(item.name ?? item.username ?? item.id)}
                    </td>
                    <td>
                      {collection === "devices" ? (
                        <CopyableCode value={id} ariaLabel="复制设备 ID" />
                      ) : (
                        <code className="mono" title={id}>
                          {adminIdentifier(collection, id)}
                        </code>
                      )}
                    </td>
                    <td>
                      {collection === "keys" ? (
                        <div className="scope-tags">
                          {(
                            (item.scopes as string[] | undefined) ?? []
                          ).map((scope) => (
                            <span key={scope} className="meta-chip mono">
                              {scope}
                            </span>
                          ))}
                        </div>
                      ) : collection === "devices" ? (
                        <span
                          className={`badge ${item.online ? "badge-running" : "badge-idle"}`}
                        >
                          {item.online ? "在线" : "离线"}
                        </span>
                      ) : collection === "users" ? (
                        <span
                          className={`badge ${item.enabled ? "badge-running" : "badge-blocked"}`}
                        >
                          {item.enabled ? "已启用" : "已停用"}
                        </span>
                      ) : (
                        <span className="mono">
                          repo #{String(item.repository_id ?? "")}
                        </span>
                      )}
                    </td>
                    <td>
                      {action ? (
                        <button
                          type="button"
                          className="btn-danger-ghost"
                          disabled={action.disabled}
                          onClick={() => remove(action.path)}
                        >
                          {action.label}
                        </button>
                      ) : null}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {modal === "create" && createLabel ? (
        <div
          className="modal-backdrop"
          onClick={() => {
            if (!busy) setModal(null);
          }}
        >
          <div
            className="modal-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="admin-modal-title"
            onClick={(e) => e.stopPropagation()}
          >
            <header className="modal-header">
              <h2 id="admin-modal-title">{createLabel}</h2>
              <button
                type="button"
                className="icon-btn"
                aria-label="关闭弹窗"
                disabled={busy}
                onClick={() => setModal(null)}
              >
                <XIcon size={16} />
              </button>
            </header>
            <form key={collection} onSubmit={create} className="modal-form">
              {collection === "projects" ? (
                <>
                  <label>
                    项目名称
                    <input
                      name="name"
                      placeholder="例如：team/repo"
                      autoFocus
                      required
                    />
                  </label>
                  <label>
                    Gitea 实例 UUID
                    <input
                      name="instance_id"
                      placeholder="与本地 Agent 配置保持一致"
                      required
                    />
                  </label>
                  <label>
                    Gitea 仓库数字 ID
                    <input
                      name="repository_id"
                      type="number"
                      min="1"
                      required
                    />
                  </label>
                </>
              ) : null}
              {collection === "users" ? (
                <>
                  <label>
                    用户名
                    <input
                      name="username"
                      autoComplete="off"
                      placeholder="输入新账户用户名"
                      autoFocus
                      required
                    />
                  </label>
                  <label>
                    初始密码
                    <input
                      name="password"
                      type="password"
                      autoComplete="new-password"
                      placeholder="设置初始登录密码"
                      required
                    />
                  </label>
                </>
              ) : null}
              {collection === "keys" ? (
                <>
                  <label>
                    设备名称
                    <input
                      name="name"
                      placeholder="例如：开发笔记本"
                      autoFocus
                      required
                    />
                  </label>
                  <label>
                    所属账户
                    <select name="user_id" required>
                      {users.map((user) => (
                        <option key={String(user.id)} value={String(user.id)}>
                          {String(user.username)}
                        </option>
                      ))}
                    </select>
                  </label>
                  <fieldset>
                    <legend>允许的操作</legend>
                    {scopes.map(([key, label]) => (
                      <label className="check" key={key}>
                        <input
                          name="scopes"
                          type="checkbox"
                          value={key}
                          defaultChecked={[
                            "read",
                            "ingest",
                            "catalog:write",
                          ].includes(key)}
                        />
                        {label}
                      </label>
                    ))}
                  </fieldset>
                </>
              ) : null}
              {modalError ? (
                <p role="alert" className="login-error">
                  {modalError}
                </p>
              ) : null}
              <footer className="modal-footer">
                <button
                  type="button"
                  className="btn-ghost"
                  disabled={busy}
                  onClick={() => setModal(null)}
                >
                  取消
                </button>
                <button className="primary" disabled={busy}>
                  {busy ? "创建中…" : "确认创建"}
                </button>
              </footer>
            </form>
          </div>
        </div>
      ) : null}

      {modal === "member" ? (
        <div
          className="modal-backdrop"
          onClick={() => {
            if (!busy) setModal(null);
          }}
        >
          <div
            className="modal-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="member-modal-title"
            onClick={(e) => e.stopPropagation()}
          >
            <header className="modal-header">
              <h2 id="member-modal-title">项目成员权限</h2>
              <button
                type="button"
                className="icon-btn"
                aria-label="关闭弹窗"
                disabled={busy}
                onClick={() => setModal(null)}
              >
                <XIcon size={16} />
              </button>
            </header>
            <form onSubmit={member} className="modal-form">
              <label>
                项目
                <select name="project_id" required>
                  {projects.map((project) => (
                    <option key={String(project.id)} value={String(project.id)}>
                      {String(project.name ?? project.id)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                账户
                <select name="user_id" required>
                  {users.map((user) => (
                    <option key={String(user.id)} value={String(user.id)}>
                      {String(user.username)}
                    </option>
                  ))}
                </select>
              </label>
              <fieldset>
                <legend>项目内允许的操作</legend>
                {scopes.map(([key, label]) => (
                  <label className="check" key={key}>
                    <input name="permissions" type="checkbox" value={key} />
                    {label}
                  </label>
                ))}
              </fieldset>
              {modalError ? (
                <p role="alert" className="login-error">
                  {modalError}
                </p>
              ) : null}
              <footer className="modal-footer">
                <button
                  type="button"
                  className="btn-ghost"
                  disabled={busy}
                  onClick={() => setModal(null)}
                >
                  取消
                </button>
                <button
                  className="primary"
                  disabled={busy || projects.length === 0 || users.length === 0}
                >
                  {busy ? "保存中…" : "保存权限"}
                </button>
              </footer>
            </form>
          </div>
        </div>
      ) : null}
    </section>
  );
}
