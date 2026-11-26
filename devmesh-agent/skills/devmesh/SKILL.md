---
name: devmesh
description: 在已配置 Gitea Token 的 DevMesh 环境中，以 backend、frontend、product 或 qa 角色通过本地 DevMesh MCP 和 Gitea 协作，创建关联 Issue、执行已认领任务、报告依赖与交付证据。用于明确的 DevMesh 工作上下文；普通代码任务不自动发布或认领 Issue。
---

# DevMesh 协作

读取本次启动器提供的角色、仓库、Issue、Work 和 Claim 上下文，以及项目自己的 AGENTS.md。身份来自本地工具，不自行填写、猜测或切换为其他任务。

1. 调用 `repository_list`（Token 可见的全部仓库，无需人工接入）、`work_status` 和只读 Gitea 工具确认任务、验收项与依赖。没有工作绑定时，通过 `work_start(repository_id, issue_number)` 请求绑定；由 Worker 验证并取得执行权后再执行。
2. 按[角色规范](references/roles/index.md)工作。只修改已授权工作区；跨角色或跨仓需求按[跨仓规则](references/cross-repository.md)创建关联任务。
3. 用项目要求的测试证明验收项。缺依赖或无法执行时调用 `work_report_blocked`，附具体原因和依赖，不编造进度。
4. 按[交付规则](references/delivery-policy.md)调用 `work_report_delivery` 上报真实证据。工具确认前不能将模型输出当交付成功；审核、合并和关闭按项目规则处理。

执行中用可见进度消息说明当前目标、关键发现和下一步。开始一组检查或改动前说明用途，测试失败或方向变化时立即说明原因和调整。消息将同步到 Relay，供用户观察和通过当前任务指令纠正方向。

用户明确要求周期检查、自动认领和开发时，按[Worker 自动执行](references/worker-automation.md)配置并启动本地轮询。在授权范围内依次认领、开分支、执行和交付，无需逐单确认。读取历史使用 Relay 授权的 Session 工具。启动和交付分别以 Worker 状态及实际证据确认。

创建任务使用 `issue_create`。按[角色判定规则](references/issue-schema.md)从任务和验收项推断 `required_role`，用户明确指定处理角色时遵从。先查询已有任务，传稳定的 `dedup_key`。用 [requirement](templates/requirement.md)、[feature](templates/feature.md)、[bug](templates/bug.md)、[test](templates/test.md) 模板组织输入。未澄清或未获自动执行授权的需求留在 backlog；通过受控 `issue_prepare` 才能进入 ready。创建和远程发布以本次用户指令及项目已授予的协作权限为边界。

[Issue 合同](references/issue-schema.md)定义输入，[工作流](references/issue-workflow.md)定义状态，[协作边界](references/collaboration-protocol.md)定义调用归属。不要直接修改正文协议块、Claim、generation、机器状态标签或人工 assignees。Push/PR 必须经过受控交付入口，不从 shell 绕过。

用户明确要求建单后自动执行时，创建请求传 `automation_requested: true`。验收项明确且依赖满足后，在同一轮调用 `issue_prepare` 使任务进入 `ready`。对本轮获明确执行授权的既有任务，可先通过 `devmesh issue authorize` 补授权，再调用准备入口。按用户指定的项目范围启用 Worker 轮询，并回读是否已认领。授权、准备和认领由工具串接，无需用户逐步操作。

Codex 无人值守模式在项目配置中显式启用。运行中收到原生审批或输入请求时，由用户通过本机 `devmesh approvals` 或 `devmesh inputs` 处理。终态 Work 只读，继续任务需返工并建立新 Work。隐私与采集规则见[共享策略](references/security-policy.md)。

仓库目录以 Gitea Token 权限为范围，不能把本地项目配置当成白名单。创建任务时携带明确的 repository_id；执行时读取目标仓库 AGENTS.md，无预设测试命令时通过 work_run_test 的 command 参数运行该仓库的实际测试，保留真实证据。
