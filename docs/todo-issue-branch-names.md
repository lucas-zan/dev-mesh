# Issue 分支命名

- **状态** 验证成功
- **基线** `main` at `177ba2e`
- **范围** Worker 为获认领的 Issue 创建 Git worktree 和开发分支。

## 行为

- 从已验证的 Issue 标题提取分支名，将中文标题转成拼音，分支名使用 ASCII 字母、数字和短横线。无可用标题时使用有文字的验收项。
- 保留 `devmesh/issue-N/` 前缀。后续认领在可读名称后附加 `--attempt-N`，用认领轮次区分分支。
- 恢复旧 Work 时继续使用持久化的分支名。Worktree 目录仍用 Claim ID 隔离。

## 验证

- [x] 先复现 Worker 为带具体标题的 Issue 使用 UUID 命名。
- [x] 验证 英文标题、中文转拼音、Git 非法字符、空标题回退、返工和 PR 交付。
- [x] 有关测试、格式化、Clippy 与 Skill 检查通过。

## 结果

- 初始回归测试观察到分支名的后缀为 Claim UUID。加入 ASCII 要求后，第二次失败确认原实现会把汉字写入分支名。
- 修正后，`cargo test --workspace --offline --locked` 通过 126 项并跳过 2 项。Cargo 格式检查、Clippy 和 4 项仓库 Skill 检查通过。
- `0.1.0-macos-arm64-issue-branch-names-20261126210243` 通过隔离安装并启用为本机版本。已核对本机 Skill 和源码一致，Worker 重启后正常响应。
