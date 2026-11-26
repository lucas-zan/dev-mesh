# Codex Worker 无人值守
- **状态** 验证成功
- **范围** 项目级 Codex 启动策略，适用于新建与恢复会话。

用户已授权指定项目无人值守执行时，项目配置可显式设置 `codex_unattended: true`。Codex 使用 `danger-full-access` 与 `never` 启动。未设置时沿用交互策略，自动认领仍由现有配置控制。其他 Runtime 拒绝此设置。

- [x] 用模拟 app-server 验证实际启动消息，覆盖默认、新建和恢复。
- [x] 用验收流程覆盖分支、测试、推送与交付。
- [x] 更新 Skill、配置说明与本机安装。

RED 记录在项目设置无人值守后仍收到 `workspace-write`。GREEN 通过 117 项 Rust 测试、Clippy、格式检查和 Skill 校验。真实 Codex app-server 回传了 `dangerFullAccess` 与 `never`，隔离交付测试使用真实 Git 推送。已更新并重启本机 Worker，生产仓库自动认领配置保持原值。
