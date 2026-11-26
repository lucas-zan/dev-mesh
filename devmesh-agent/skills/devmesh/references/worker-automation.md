# 周期检查与自动开发

用户明确要求自动检查和开发已授权项目时，使用本地 Worker 的轮询机制。Skill 提供操作流程，持续调度由 DevMesh daemon 执行。

1. 确认项目已配置并安装所选 Runtime、Gitea MCP；先运行 `devmesh doctor` 查看状态。真实凭据连接验证使用 `devmesh doctor --verify`，它不等于完整模型任务验收。
2. 在配置的 `projects` 中，按真实仓库 ID 为获授权的项目启用 `auto_claim`。检查 `worker.enabled`、`worker.poll_interval_seconds`、`worker.max_concurrency`，以及项目的 `roles`、`default_branch`、`concurrency`。使用用户给定的范围和参数，其他设置沿用现有值。用户仅询问能力时保持配置和运行状态。
3. 首次运行用 `devmesh worker start`。修改配置后，确认在途 Work 已收尾，再停止并重启 Worker 以载入新配置。用 `devmesh worker status` 确认可连接且未暂停。启动失败时报告实际错误。
4. Worker 只认领 open、ready、允许自动执行、角色匹配、标签一致、依赖满足且没有冲突 Claim 的任务；通过 CAS 取得 Claim 后创建独立 worktree，再启动配置的 Runtime。不要另写模型循环绕过 Worker。
5. `devmesh worker pause` 暂停新认领，在途 Work 保持执行；`resume` 恢复认领；`stop` 请求停止受管工作并收尾。暂停不能描述成已取消在途工作。

Codex 项目经用户明确授权无人值守和 Full Access 后，在该项目对象内设置 `"codex_unattended": true`。Worker 会以 `danger-full-access` 和 `never` 启动或恢复该项目的 Codex 会话。默认项目继续使用 `workspace-write` 和 `on-request`，新发现的仓库也沿用默认策略。此模式与自动认领独立，交付仍走受控工具。

收到原生审批或输入请求时，Work 会等待并继续占用槽位。使用 `devmesh approvals list --work UUID` 查看当前请求。单问题输入使用 `devmesh inputs reply --work UUID --id UUID --text TEXT`。Codex 多问题输入使用 `--answers-json '{"question_id":["answer"]}'`，问题 ID 来自原生请求。已超时的终态 Work 需要依照 Issue 工作流进入新一轮执行。

当前调度支持“每隔 N 秒检查”，不支持日历表达式、指定每天几点或开机自启。daemon 需要机器保持运行；没有经过系统服务托管时，不承诺退出宿主、注销或重启后仍持续执行。当前执行环境若禁止本地 socket、进程检查或网络，应明确未能启动/验收，不声称后台任务已运行。

历史记录使用 `session_list`、`session_get`、`session_events`，由 Relay 校验读取权限。没有配置 Relay 时明确无法读取历史；不把本地 Work 状态当成 Session 历史。
