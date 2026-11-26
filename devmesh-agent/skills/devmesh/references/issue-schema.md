# Issue 输入

`issue_create` 输入：repo_ref（instance_id、repository_id）、title、description、required_role、type、priority、acceptance_criteria、dependencies、dedup_key、automation_requested。creator_role 与执行角色分离，真实用户身份和 work 上下文由工具获取。

每项 acceptance_criteria 含稳定 id 与可验证描述。dependency 指明 issue_ref、条件（issue_closed/pr_ready/artifact_ready/pr_merged）与候选版本或 selector。工具在 backlog 中创建任务并补全 issue_ref、元数据、标签；角色、验收和依赖满足且有授权后才能 ready。

正文使用唯一 `<!-- devmesh:issue ... -->` 版本化 JSON 块，模型不直接编辑。缺字段、标签冲突或非 DevMesh Issue 不能自动接管。完整机器结构由随包发布的 devmesh-protocol Schema 定义，不在 Skill 中复制。


## 处理角色

创建 Issue 时，由 Agent 按主要交付物和验收标准选择一个 `required_role`。用户明确指定处理角色时，优先采用其指定的有效角色。未指定时直接推断，无需用户确认角色。Issue 类型、发布者角色和当前 Worker 角色都不决定处理角色。

- `backend`：主要交付 API、服务、数据模型、数据库或服务端集成。
- `frontend`：主要交付页面、界面交互或客户端行为。
- `product`：主要交付需求澄清、业务规则、范围或验收标准。
- `qa`：主要交付独立验证、回归测试或验收报告。实现任务附带的必要测试仍属于该实现任务。

标题或类型不够明确时以实际验收项为准。例如“登录按钮无法点击”的 bug 归 `frontend`，而“验证登录超时与错误提示是否符合验收标准”归 `qa`。跨角色且交付可独立验收时，按[跨角色与跨仓规则](cross-repository.md)拆分，每条分别判断角色。
