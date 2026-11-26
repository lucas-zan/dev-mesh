# DM-07: Pi Durable 标准 Host

- **Todo ID**: DM-07
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
Pi Durable 标准 Host；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-015`, `REQ-016`, `REQ-174`, `REQ-175`, `REQ-176`, `REQ-177`, `REQ-178`, `REQ-179`, `REQ-180`, `REQ-181`, `REQ-182`, `REQ-183`, `REQ-184`, `REQ-185`, `REQ-186`, `REQ-187`, `REQ-188`, `REQ-189`, `REQ-190`, `REQ-191`, `REQ-192`, `REQ-193`, `REQ-194`, `REQ-195`, `REQ-196`, `REQ-197`, `REQ-198`, `REQ-199`, `REQ-200`, `REQ-201`, `REQ-202`, `REQ-203`, `REQ-204`, `REQ-205`, `REQ-206`, `REQ-207`, `REQ-208`, `REQ-209`, `REQ-210`, `REQ-211`, `REQ-411`, `REQ-453`, `REQ-454`, `REQ-455`, `REQ-456`, `REQ-457`, `REQ-458`, `REQ-459`, `REQ-460`, `REQ-461`, `REQ-462`, `REQ-463`, `REQ-464`, `REQ-465`, `REQ-466`, `REQ-467`, `REQ-468`
- **Produces**: `artifact:durable` — devmesh-agent/packages/pi-durable-host
- **Gates**: npm test --workspace @devmesh/pi-durable-host
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` 锁定上游 SDK 与持久化 Host。
  - **Test and RED**: 在 devmesh-agent/packages/pi-durable-host/src/；test/host.test.ts 对应测试中新增以下行为：固定真实上游包与存储；Harness/Conversation start/observe/submit/resume；工具在授权工作目录运行。运行 `npm test --workspace @devmesh/pi-durable-host`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 固定真实上游包与存储；Harness/Conversation start/observe/submit/resume；工具在授权工作目录运行。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/packages/pi-durable-host/src/；test/host.test.ts；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `npm test --workspace @devmesh/pi-durable-host`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:durable` 增加“锁定上游 SDK 与持久化 Host”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` 忙碌注入、取消和恢复证据。
  - **Test and RED**: 在 devmesh-agent/packages/pi-durable-host/src/；test/host.test.ts 对应测试中新增以下行为：busy steer、idle reject；停止根/子任务确认静止；崩溃恢复不重复外部副作用，报告实际能力。运行 `npm test --workspace @devmesh/pi-durable-host`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: busy steer、idle reject；停止根/子任务确认静止；崩溃恢复不重复外部副作用，报告实际能力。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/packages/pi-durable-host/src/；test/host.test.ts；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `npm test --workspace @devmesh/pi-durable-host`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:durable` 增加“忙碌注入、取消和恢复证据”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## Evidence 2026-11-25T12:43:01.852346+00:00

RED: 缺 Host；GREEN: 真实 @earendil-works/pi-durable 1.6.0 SDK + Node SQLite 的 2 个持久化/idle reject/绑定测试通过，tsc 构建通过。通过原生 whenIdle reject、ToolTask signal gate 实现 busy steer 与本地审批。真实模型及工具审批 E2E 留 DM-12。

## 验收审计

实际 SDK SQLite/子进程 IPC 测试通过；真实模型审批、子任务停止未验收。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
