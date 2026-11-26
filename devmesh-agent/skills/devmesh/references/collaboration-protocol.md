# 协作边界

Gitea 是任务、Claim、Git 和 PR 的事实来源。本地 Worker 负责确定性的认领、Workspace、执行状态和交付校验；Relay 接收事件和转发控制，不分配任务。

模型通过官方 Gitea MCP 读取，写入通过 DevMesh MCP facade。创建、评论、交付前校验授权目标；执行中的交付还需校验当前 Claim。`issue_claim` 是对 Worker 的请求，不是自行写锁。`issue_release` / `work_finish` 是停止与收尾请求，只有静止确认后才能释放。真实执行实例未知时保留 Claim，由本地恢复流程处理。

网络超时可能已产生副作用；用相同 operation_id 查询结果，不重新提交不同请求 ID。工具返回 conflict、unknown 或 recovery_required 时停止该外部操作，报告实际状态。普通工作评论走 Gitea；Web Comment 仅保存在 Relay，不会进入 Agent 上下文。
