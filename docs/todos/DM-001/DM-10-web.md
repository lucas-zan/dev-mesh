# DM-10: Web 工作台

- **Todo ID**: DM-10
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
Web 工作台；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-022`, `REQ-023`, `REQ-345`, `REQ-346`, `REQ-347`, `REQ-348`, `REQ-349`, `REQ-350`, `REQ-351`, `REQ-469`, `REQ-470`, `REQ-471`, `REQ-472`, `REQ-473`, `REQ-474`, `REQ-475`, `REQ-476`, `REQ-477`, `REQ-478`, `REQ-479`, `REQ-480`, `REQ-481`, `REQ-482`, `REQ-483`, `REQ-484`, `REQ-485`, `REQ-486`, `REQ-487`, `REQ-488`, `REQ-489`, `REQ-490`, `REQ-491`, `REQ-492`, `REQ-493`, `REQ-494`, `REQ-495`, `REQ-496`, `REQ-497`, `REQ-498`, `REQ-499`, `REQ-500`, `REQ-501`, `REQ-502`
- **Produces**: `artifact:web` — devmesh-relay/web
- **Gates**: npm test --workspace @devmesh/web
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` 项目树与 Session 工作记录。
  - **Test and RED**: 在 devmesh-relay/web/src/；src/workspace.test.ts 对应测试中新增以下行为：项目/Issue 搜索筛选、运行与历史选择、消息/工具/状态、SSE 重连与分页、脱敏标记。运行 `npm test --workspace @devmesh/web`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 项目/Issue 搜索筛选、运行与历史选择、消息/工具/状态、SSE 重连与分页、脱敏标记。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-relay/web/src/；src/workspace.test.ts；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `npm test --workspace @devmesh/web`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:web` 增加“项目树与 Session 工作记录”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` 授权控制与管理入口。
  - **Test and RED**: 在 devmesh-relay/web/src/；src/workspace.test.ts 对应测试中新增以下行为：明确 Session 目标；steer/follow-up/prompt/comment 和真实回执；本地审批提示；账号项目设备管理。运行 `npm test --workspace @devmesh/web`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 明确 Session 目标；steer/follow-up/prompt/comment 和真实回执；本地审批提示；账号项目设备管理。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-relay/web/src/；src/workspace.test.ts；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `npm test --workspace @devmesh/web`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:web` 增加“授权控制与管理入口”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## 验收审计

Web 模型/SSR 测试和 Vite 构建通过；虚拟列表、快照续订和浏览器交互仍未完成。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
