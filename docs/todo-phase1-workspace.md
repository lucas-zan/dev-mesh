# Todo: 第一期自动工作区

- **Status**: 验证成功
- **Scope**: 项目缺失时显式 auto_clone；QA 使用依赖固定的候选 SHA，而非默认分支。

## Behavior

- 缺目录且 auto_clone=false 时认领前报错；true 时克隆配置的 remote。已有目录不覆盖。
- QA 从 dependency_snapshot 选择本仓库的单一 PR candidate，fetch 固定 SHA 后创建独立 worktree，测试与报告必须针对该 SHA。
- 克隆/依赖解析失败不留下刚取得的 Claim 或本地活跃槽位；已有人工修改保留。

## Test first

- 实际本地 Git remote：克隆、复用、禁止克隆时失败、QA candidate 与 main 不同。
- QA 无歧义候选选择；报告提交其他候选时拒绝。
- RED: 当前无 auto_clone 及 QA revision 选择，所有角色都从默认分支启动。

## Implementation

配置布尔值默认 false；工作区预检在 Claim 前；已认领后的准备错误沿用 blocked/释放。只使用实际 Git 与已有 Gitea Adapter。

## Verification

Worker/Git/QA 聚焦测试、Gitea 回归、fmt/Clippy。

## Done

- [x] RED 已观察。
- [x] 实现与回归通过。

## Evidence 2026-11-26T01:44:54.719948+00:00

- RED: 缺少 auto_clone 与 QA candidate_revision；原代码统一使用默认分支。
- GREEN: 实际 Git 克隆/复用、QA 固定候选、准备失败收尾与相关回归通过。
- Gate: `python3 scripts/verify.py`，本地 Rust/TS/构建/安装检查通过；`python3 scripts/test_e2e.py` 通过。
- Scope: 实际 Git、测试命令、SQLite、子进程；Gitea/模型仍是测试替身，真实验收未执行。
