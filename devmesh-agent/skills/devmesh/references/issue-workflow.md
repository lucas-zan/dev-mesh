# 工作流

backlog → ready → claimed → running → review → done。需求/QA 等非代码任务可按授权规则从 running 验收至 done。

创建任务并不授权自动开发；`issue_prepare` 由授权人或已授权策略调用。认领前依赖必须满足，CAS 成功后再启动。机器从不设置或删除 assignee；人工名单仅限制领取资格。

阻塞调用 `work_report_blocked`，Worker 先确认 Runtime/工具停止，再 CAS 写 blocked 并释放 Claim。审批/输入等待不等于业务 blocked，不自行释放。Review 失败调用 `issue_request_rework`，记录原候选 SHA、失败项和原因；重新 ready 后新 Claim generation、新 Work，不恢复终态 Work。

一次 Turn 完成不等于 Issue 完成。短暂 idle 可以接收 Prompt；交付收尾和终态拒绝新 Prompt/Steer/Follow-up。模型不能因回复“完成”直接关闭 Issue。
