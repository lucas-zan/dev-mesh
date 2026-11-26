# DM-001 DevMesh V1.1 implementation plan

## Scope
以 devmesh-v1.1 为功能基线；用户更新优先：服务端名 devmesh-relay，App 延后。本期先本地 Skill/Gitea，再三 Runtime/安装器，再 Relay/Web。真实 token/key 和模型服务验收最后执行；测试服务和合成凭据可先验证代码。

## Execution
本地开发采用版本化 Cargo workspace + npm workspaces，生产交付包独立。先行为测试观察失败，再实现，验收证据写入 todo；不以模拟测试冒充真实上游支持。

## Work packages
### DM-01 本地协作 Skill
角色化协作与跨仓需求：根据角色读取规范，调用受控 MCP 创建/评论/报告；模型不直接修改 Claim 或决定交付完成；需求、缺陷、测试模板与交付示例：模板携带验收项、执行角色、依赖和证据；明确 blocked/rework 的操作入口。
### DM-02 公共协议与领域类型
冻结协议结构与状态机：复用 V1.1 schema；Rust/TS 共享 UUID、字符串序号、Issue/Work/Session 和命令状态；非法转换返回显式错误；REST/WSS/SSE 合同与幂等身份：OpenAPI/AsyncAPI 明确注册快照与事件增量；同 frame 重试幂等，新实例注册是新 frame，不覆盖历史实例。
### DM-03 Gitea 受控本地 MCP
Gitea API 与官方 MCP 边界：配置用户 endpoint 和凭据引用，真实用户身份/仓库 ID；只读官方 MCP 与窄 CAS API 分离；创建、准备、认领、释放及返工：正文唯一 JSON 块初始化 backlog，标签一致才 ready；CAS 409/未知不启动；不修改 assignee；generation 增长；本地 stdio MCP 与交付写入：JSON-RPC tools/list/call；绑定可信 Work 上下文；证据/评论/PR 经 Worker 校验 Claim 和目标版本后执行。
### DM-04 本地 Worker、Worktree 与 Bridge
本地执行状态、并发和 Worktree：预留槽再 CAS，固定基线，专用 worktree，不改人工目录；等待审批/输入占槽，终态静止后释放；事件、控制队列和持久化恢复：消息先脱敏再 SQLite；sequence 连续；连续 ACK 后清理；过期/旧实例/终态指令拒绝；unknown 不重放；按角色验证真实交付证据：PR 候选 SHA、不可变产物、AC 测试和 QA 对象绑定；review 释放，merge 后 reconcile；缺证据 blocked。
### DM-05 Codex 原生适配
App Server 受管生命周期及事件：initialize/thread/turn、JSON-RPC request/response；事件映射、真实 native ID；审批请求本地处理；同一活动 Turn 控制与恢复：steer expectedTurnId，idle 拒绝；中断等到静止；resume 原生线程、新实例使旧命令失效。
### DM-06 Pi 原生适配
Pi RPC 与 Extension 包：RPC prompt/get_state/message/tool 生命周期归一；Package 分发 Skill/Extension，stdout 只传协议；Steer、本地队列与输入审批：busy/epoch 串行校验；follow-up 本地持久队列；native disposition 与 applied 证据分离；原生 UI 请求转本机。
### DM-07 Pi Durable 标准 Host
锁定上游 SDK 与持久化 Host：固定真实上游包与存储；Harness/Conversation start/observe/submit/resume；工具在授权工作目录运行；忙碌注入、取消和恢复证据：busy steer、idle reject；停止根/子任务确认静止；崩溃恢复不重复外部副作用，报告实际能力。
### DM-08 三 Runtime 一键安装与 CLI
统一 install/init/doctor：检测 OS/runtime 版本；安装本工具包、Skill、官方 MCP 与三适配；合并配置，doctor 运行实际握手；升级、回滚和卸载：manifest 只记录自有文件；校验发布哈希、备份用户配置、失败回滚；卸载保留用户数据。
### DM-09 Relay 持久服务与 API
PostgreSQL、账号和项目设备：SQL 迁移；bootstrap、用户/ACL、Key/设备、Gitea 仓库映射；测试凭据不等于真实环境验证；WSS 采集与 REST/SSE 查询：注册归属、连续 ACK、幂等事件；事务写投影/feed；快照游标续订；Catalog publisher 与版本冲突；远程指令与真实回执：命令持久化后路由当前设备；TTL/权限/能力校验；回执历史与unknown；Comment 独立存储。
### DM-10 Web 工作台
项目树与 Session 工作记录：项目/Issue 搜索筛选、运行与历史选择、消息/工具/状态、SSE 重连与分页、脱敏标记；授权控制与管理入口：明确 Session 目标；steer/follow-up/prompt/comment 和真实回执；本地审批提示；账号项目设备管理。
### DM-11 跨项目离线集成与发布制品
真实本地服务集成：测试 Gitea HTTP 服务与受管假 Runtime 连通 Agent/Relay/Postgres/Web；20 Worker 同抢、断线重传、旧指令拒绝；构建部署与全新目录安装：Rust/TS lint/test/build、Compose 部署、安装 tar/哈希；全新临时 HOME install/doctor/update/uninstall，文档明确验证范围。
### DM-12 最终真实凭据与上游验收
真实 Gitea/Relay 身份与并发验收：由用户提供实际 URL 和凭据引用后，读真实用户/仓库；在专用测试 Issue 做 CAS 与生命周期；不在代码或日志存秘密；三个 Runtime 真机安装与任务控制：固定版本，真实模型工作、同 Session steer、审批、停止与恢复；逐项记录证据，缺 key 明确未验证。

## Done
本期非 App 功能、所有节点的实际测试及构建通过；真实环境未验证部分如实留为待验收，不标记全计划完成。

## STOP
真实凭据只在最终验收使用；不泄露、不伪造回执。上游接口不支持时明确未支持；未知副作用不重放。App 不实现。
