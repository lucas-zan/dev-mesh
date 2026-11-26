# Todo: 显式授权已有 Issue 给 Agent 执行

- **Status**: 验证成功
- **Scope**: 为已创建的 manual-only Issue 提供受控授权入口，不改认领规则。

用户已明确授权处理 Issue #5。输入为 `devmesh issue authorize --number N`，沿用目标仓库 can_prepare 权限；输出 automation_allowed=true，并替换 agent 标签。保留正文、角色、assignees 和状态。重复授权幂等；活动 Claim 拒绝修改。

先在 gitea.rs 测试未授权任务转为可认领、重复调用不增加版本、活动 Claim 冲突；在 cli.rs 测试帮助入口。预期 RED 为缺少 authorize 方法。实现 TaskService 窄入口和 CLI 路由，验证聚焦测试、fmt、Clippy。此轮不新增调度能力。

验证：缺少 authorize 方法时编译失败（RED）；Gitea 11 项与 CLI 4 项聚焦测试通过（GREEN），fmt 和 Clippy 通过。通过新入口授权真实 Issue #5，保留既有角色、验收项及状态后，再由 issue_prepare / work_start 正常认领。
