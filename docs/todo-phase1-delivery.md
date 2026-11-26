# Todo: 第一期交付与关闭闭环

- **Status**: 验证成功
- **Scope**: 实际测试证据 → PR → review → 人工合并 → done；非代码任务人工验收。

## Behavior

- Worker 每轮检查本项目已交付的代码 Issue，只有实际 merged 且候选 SHA、目标分支相符时进入 done。
- done 写入后标签/close 失败，再执行时继续修复，不重复推进 state_rev；已关闭的 Issue 不重复 close。
- product/qa 已释放的 review 任务可以通过显式 `issue accept` 验收；先校验交付与不可变报告，QA 还要核对当前候选。
- report 测试失败或 candidate 不符不能完成；未知操作不盲重放。

## Test first

- 未合并、实际已合并、候选/目标变化、关闭失败后重试、已关闭情况。
- 非代码缺少交付不能 accept；有效报告可完成。
- RED: 当前 reconcile 不能重试 done，Worker 不自动检查合并，CLI 没有 accept。

## Implementation

沿用现有状态 CAS 和 status 投影，提取完成关闭步骤；周期调度复用现有轮询，不新增后台服务。

## Verification

真实 Git + 注入 Gitea 的完整 Work/交付测试；相关 Worker/Runtime 回归；fmt/Clippy。

## Done

- [x] RED 已观察。
- [x] 实现与回归通过。

## Evidence 2026-11-26T01:44:54.719948+00:00

- RED: done 后重试 reconcile 失败；无自动合并确认与 accept；QA 报告提交导致 HEAD 不等于被测 SHA；无测试也能报告 QA 通过。
- GREEN: 代码完整交付/merge/关闭、QA 独立报告、20 Worker 单 Runtime、关闭修复与 CLI 后台启动测试通过。
- Gate: `python3 scripts/verify.py`，本地 Rust/TS/构建/安装检查通过；`python3 scripts/test_e2e.py` 通过。
- Scope: 实际 Git、测试命令、SQLite、子进程；Gitea/模型仍是测试替身，真实验收未执行。
