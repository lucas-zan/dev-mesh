# 第一期开发与验收状态

当前范围以用户于 2026-11-26 确认的[第一期功能合同](phase1-functional.md)为准：先走通功能，不扩展安全治理。服务名称为 **devmesh-relay**，**App 延后**。

## 已实现与本地验证范围

- 本地 Skill、受控 Gitea MCP、Issue 创建/修复/准备、Claim 并发认领/释放/返工。
- Worker 周期轮询、角色与依赖检查、可选 auto_clone、独立 Git worktree、槽位管理和失败收尾。
- Codex App Server、Pi RPC Extension、Pi Durable SDK Host；本地审批/多问题输入、命令去重、等待超时和恢复状态。
- 真实测试命令、Push、PR 与交付报告；代码进入 review 后由人工合并，Worker 轮询确认实际候选/目标/merge 结果后 done/close；关闭失败可继续修复。
- QA 从依赖指定的候选 SHA 建立工作区并执行测试；报告提交可以有独立 commit，人工 `issue accept` 验收后完成。
- Relay 项目/账号/设备、REST/WSS/SSE、事件持久化与 ACK、Catalog、指令路由和回执。
- Web 工作记录、当前/历史 Session、命令与回执、设备上下线、分页、虚拟列表、快照游标和连续 SSE 刷新。
- 安装/更新/回滚/卸载、发布包校验、配置与 doctor。

实际本地结果见 `verification/latest.json`、`verification/release.json`。集成测试包括真实 Git、测试命令、SQLite、子进程管道、Pi Durable SDK，以及内存传输的 Agent/Relay 帧。`functional_flow` 使用 Gitea 与模型响应测试替身，不能冒充真实上游验收。

新增集成覆盖：空目录克隆 → Runtime 子进程修改并提交 → 实际测试 → Push/PR → review → Git merge → done/close；QA 候选测试 → 独立报告提交 → 人工验收；20 个 Worker 只启动一个 Runtime 子进程。

## 第一期尚未完成的真实验收

| 环节 | 已有证据 | 仍需实际执行 |
| --- | --- | --- |
| Codex 0.172.0 | 映射/审批/输入、管道与测试替身闭环 | 实际模型任务、Steer、审批、中断与恢复 |
| Pi 1.6.0 | Extension、命令/事件 Schema | 实际安装、RPC 模型任务、队列/UI 和恢复 |
| Pi Durable 1.6.0 | 实际 SDK/SQLite/Host 管道、停止与超时 | 模型任务、活动工具审批、控制与恢复 |
| Gitea MCP v1.13.0 | Adapter/协议映射、CAS 行为测试 | 实际二进制、服务器 content_version CAS、真实仓库交付 |
| Relay/PostgreSQL | Repository/Router、内存传输与 SQL 迁移代码 | 真实迁移、TCP/WSS/SSE、部署与持久化 |
| Web | 模型/SSR 测试和 Vite 构建 | 浏览器视觉、滚动、交互、断线重连 |
| 一键安装 | 隔离离线安装/更新/回滚/卸载 | 在线安装三 Runtime/MCP 和实际机器 doctor |

`verification/live.json` 是当前真实环境预检结果：TCP/Unix 监听和进程检查被执行环境禁止，doctor 因无配置未执行。它的 passed=false；没有将这些项目记为通过。

验收步骤见 [live-validation.md](live-validation.md)。`validate_live.py` 可以执行真实预检，并通过显式 `--run-issue` 启动专用 Issue 的模型开发；其报告明确不覆盖浏览器、原生控制/恢复与人工合并确认。需要真实地址、专用仓库、本地配置/凭据引用，以及允许这些操作的开发环境。真实 Token/Key 仍放最后使用。

## 本期不作为完成条件

邀请体系、完整审计、细分权限治理、通用安全加固、全嵌套输入安全矩阵、SQLx/Argon2id 选型替换均不纳入第一期阻塞项。当前保留 psql Adapter 与 PBKDF2。已有能力不因缩小范围而顺手拆除。

公共 Schema 的 evidence.artifact_key 可选字段用于命名产物依赖；App 后续直接使用 Relay REST/SSE。

## 一键发布与人工安装入口

`make -C devmesh-relay` 自动构建和部署 PostgreSQL + Relay/Web，生成并保留部署配置，首次 bootstrap，提供 status/logs/down/credentials。默认对外端口 17843。

`make install` 构建独立本机版本，安装 Runtime/官方 MCP，为 Codex 注册 Skill/MCP；Pi/Durable 由 Worker 自动加载。配置示例随包交付，已有项目配置不覆盖。

验证记录见 `verification/deploy-install.json`；说明见 [发布说明](../devmesh-relay/README.md)、[本地安装与人工测试](manual-install-test.md)。本轮实际安装验证使用 offline 与隔离配置，不代表上游联网安装通过。
