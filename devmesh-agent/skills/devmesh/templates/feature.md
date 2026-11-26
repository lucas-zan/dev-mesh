# feature 输入模板

提交给 `issue_create` 的结构化输入；身份、Claim 和协议块由本地工具生成。

- title：具体可观察目标
- description：背景、范围、约束；Bug 附复现步骤/实际与预期；Test 附被测候选 SHA 和环境
- required_role：由 Agent 按[处理角色规则](../references/issue-schema.md)填写。
- type：feature
- priority：p1（按项目规则选择）
- acceptance_criteria：带稳定 ID 的输入、操作、期望结果；Bug 包含回归条件
- dependencies：完整 issue_ref + 条件 + 不可变版本/selector；无依赖用空数组
- dedup_key：来源任务 + 目标仓库 + 类型 + 业务标识
- automation_requested：仅明确授权时 true；否则 false

已有同类任务时更新证据，不重复创建。创建后默认 backlog；授权准备流程决定何时 ready。
