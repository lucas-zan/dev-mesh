# DM-05: Codex 原生适配

- **Todo ID**: DM-05
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
Codex 原生适配；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-011`, `REQ-012`, `REQ-174`, `REQ-175`, `REQ-176`, `REQ-177`, `REQ-178`, `REQ-179`, `REQ-180`, `REQ-181`, `REQ-182`, `REQ-183`, `REQ-184`, `REQ-185`, `REQ-186`, `REQ-187`, `REQ-188`, `REQ-189`, `REQ-190`, `REQ-191`, `REQ-192`, `REQ-193`, `REQ-194`, `REQ-195`, `REQ-196`, `REQ-197`, `REQ-198`, `REQ-199`, `REQ-200`, `REQ-201`, `REQ-202`, `REQ-203`, `REQ-204`, `REQ-205`, `REQ-206`, `REQ-207`, `REQ-208`, `REQ-209`, `REQ-210`, `REQ-211`, `REQ-411`, `REQ-453`, `REQ-454`, `REQ-455`, `REQ-456`, `REQ-457`, `REQ-458`, `REQ-459`, `REQ-460`, `REQ-461`, `REQ-462`, `REQ-463`, `REQ-464`, `REQ-465`, `REQ-466`, `REQ-467`, `REQ-468`
- **Produces**: `artifact:codex` — devmesh-agent/src/runtime/codex.rs
- **Gates**: cargo test -p devmesh-agent --test codex
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` App Server 受管生命周期及事件。
  - **Test and RED**: 在 devmesh-agent/src/runtime/{process,codex}.rs；tests/codex.rs 对应测试中新增以下行为：initialize/thread/turn、JSON-RPC request/response；事件映射、真实 native ID；审批请求本地处理。运行 `cargo test -p devmesh-agent --test codex`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: initialize/thread/turn、JSON-RPC request/response；事件映射、真实 native ID；审批请求本地处理。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/runtime/{process,codex}.rs；tests/codex.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test codex`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:codex` 增加“App Server 受管生命周期及事件”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` 同一活动 Turn 控制与恢复。
  - **Test and RED**: 在 devmesh-agent/src/runtime/codex.rs；tests/codex.rs 对应测试中新增以下行为：steer expectedTurnId，idle 拒绝；中断等到静止；resume 原生线程、新实例使旧命令失效。运行 `cargo test -p devmesh-agent --test codex`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: steer expectedTurnId，idle 拒绝；中断等到静止；resume 原生线程、新实例使旧命令失效。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/runtime/codex.rs；tests/codex.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test codex`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:codex` 增加“同一活动 Turn 控制与恢复”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## Evidence 2026-11-25T12:43:01.792923+00:00

RED: 无 Runtime 模块；GREEN: Codex native request/事件/审批 2 测试与 stdio 真子进程相关性测试通过。schema 来自本机 codex-cli 0.172.0 导出；真实模型/Steer E2E 不计入本节点的离线通过证据，仍须 DM-12 验收。

## 验收审计

Codex 4 项聚焦测试及共享 wire 校验通过；真实 App Server/模型/审批/恢复未验收。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
