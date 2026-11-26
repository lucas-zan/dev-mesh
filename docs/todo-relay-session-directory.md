# Relay 按会话形成项目目录

- **Status**: 完成

Worker 继续在本地发现仓库和 Issue，并在目标仓库的独立 Git worktree 中运行。Relay 不依赖仓库目录预同步。首个实际 Work 上传仓库和 Issue 标识及展示信息，Relay 复用或建立对应记录，保存 Session 后工作台才显示该 Issue 和项目，历史保留。既有项目权限与设备归属继续执行。管理界面保留原始记录。

先用双工链路测试无预设项目时上传实际 Work、Session 和事件，再核对工作台查询只返回有会话的项目与 Issue，包含历史会话。

实现：取消 Worker 全仓库目录同步；实际 Work 携带仓库名、Issue 标识、标题和链接，Relay 建立关联；工作台使用 `with_sessions=true` 查询并隐藏空组。Session 注册发出目录变更事件，页面自动更新。连接时丢弃旧目录同步帧和过时状态。

本地执行：Worker 从 Issue 对应仓库创建独立 Git worktree，Runtime 的 cwd 指向该 worktree。Relay 经 Work 为 Session 保存 project_id 和 issue_ref，不依赖本地路径推断归属。

验证：Rust 工作区测试通过（114 passed、2 ignored），协议 19 项及客户端 3 项、Web 16 项测试通过，Web 构建、API 合同检查和 Clippy 通过。补充双工集成测试验证旧同步状态清理，先失败后通过。本机 Relay/Web 已部署，Worker 空闲后更新重启，认证查询确认设备在线、工作台项目数为 0；未创建或修改真实 Gitea Issue。
