# 交付

backend/frontend：提交仓库、base/head SHA、目标分支、PR、AC 对应测试命令/exit code/报告、必要接口契约；frontend 增加构建和必要交互验证。Worker 验证本地提交与远程候选后才进入 review。

product：不可变需求文档、验收项、子任务和业务验收记录。qa：被测精确 SHA/产物、环境版本、每项 AC 的 pass/fail/skipped、不可变报告、缺陷关联；skipped 不等于通过。

测试证据说明实际执行结果，不能把计划、模拟输出或模型自评当测试。H1 的 QA/审核不能继承给 H2。默认由人工在 Gitea 审核和合并；自动策略须显式授权。Worker 回读真实 merge/交付证据后才协调 done/close。

第一期代码交付顺序：完成修改与 Git commit，调用 `work_run_test` 取得该提交的实际测试证据，再调用 `work_push`、`work_create_pr`、`work_report_delivery`。`work_push` 返回 `sha`，作为本次候选版本；不要把未提交修改的测试结果当作该 SHA 的证明。报告引用原样返回的测试证据和实际 PR。

QA 先在依赖指定的候选 SHA 运行 `work_run_test`，再生成、提交并通过 `work_push` 发布不可变报告。报告 artifact 的 commit_sha 可以是报告提交；delivery.candidate_sha 和测试证据仍是原被测 SHA。生产候选变化必须复测。QA/product 进入 review 后，由用户执行 `devmesh issue accept --number N` 验收关闭；模型不自行代替用户验收。代码任务由人工合并 PR 后，Worker 自动确认 done；也可由用户执行 `devmesh issue reconcile --number N`。

代码交付的 PR 证据使用下列结构，仓库、PR 编号、候选 SHA 与目标分支取真实工具结果。PR 字段放在嵌套的 `pr` 对象内。

```json
{
  "evidence_id": "<new UUID>",
  "kind": "pr",
  "candidate_sha": "<work_push sha>",
  "outcome": "pass",
  "summary": "Reviewed change candidate",
  "acceptance_ids": ["AC1"],
  "pr": {
    "repository": {"instance_id": "<instance UUID>", "repository_id": 1},
    "number": 5,
    "head_sha": "<work_push sha>",
    "target_branch": "<configured base branch>"
  }
}
```

将当前 Work 中通过的 `work_run_test` 原始返回证据与上述 PR 证据放入 `evidence`。`acceptance_results` 每项使用 `{"id":"AC1","outcome":"pass","evidence_ids":["<对应证据 UUID>"]}`。失败测试在 PR 中说明具体范围与影响，通过复测后引用新的通过证据。错误返回时检查 `error.code` 和字段结构，按已发布合同修正并重试。
