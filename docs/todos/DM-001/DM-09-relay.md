# DM-09: Relay 持久服务与 API

- **Todo ID**: DM-09
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
Relay 持久服务与 API；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-019`, `REQ-020`, `REQ-021`, `REQ-283`, `REQ-284`, `REQ-285`, `REQ-286`, `REQ-287`, `REQ-288`, `REQ-289`, `REQ-290`, `REQ-291`, `REQ-292`, `REQ-293`, `REQ-294`, `REQ-295`, `REQ-296`, `REQ-297`, `REQ-298`, `REQ-299`, `REQ-300`, `REQ-301`, `REQ-302`, `REQ-303`, `REQ-304`, `REQ-305`, `REQ-306`, `REQ-307`, `REQ-308`, `REQ-309`, `REQ-310`, `REQ-311`, `REQ-312`, `REQ-313`, `REQ-314`, `REQ-315`, `REQ-316`, `REQ-317`, `REQ-318`, `REQ-319`, `REQ-320`, `REQ-321`, `REQ-322`, `REQ-323`, `REQ-324`, `REQ-325`, `REQ-326`, `REQ-327`, `REQ-328`, `REQ-329`, `REQ-330`, `REQ-331`, `REQ-332`, `REQ-333`, `REQ-334`, `REQ-335`, `REQ-336`, `REQ-337`, `REQ-338`, `REQ-339`, `REQ-340`, `REQ-341`, `REQ-342`, `REQ-343`, `REQ-344`, `REQ-352`, `REQ-353`, `REQ-354`, `REQ-355`, `REQ-356`, `REQ-357`, `REQ-358`, `REQ-359`, `REQ-360`, `REQ-361`, `REQ-362`, `REQ-363`, `REQ-364`, `REQ-365`, `REQ-366`, `REQ-367`, `REQ-368`, `REQ-369`, `REQ-370`, `REQ-371`, `REQ-405`, `REQ-424`, `REQ-425`, `REQ-426`, `REQ-427`, `REQ-428`, `REQ-429`, `REQ-430`, `REQ-431`, `REQ-432`, `REQ-433`, `REQ-434`, `REQ-435`, `REQ-436`, `REQ-437`, `REQ-438`, `REQ-439`, `REQ-440`, `REQ-441`, `REQ-442`, `REQ-443`, `REQ-444`, `REQ-445`, `REQ-446`, `REQ-447`, `REQ-448`, `REQ-449`, `REQ-450`, `REQ-451`, `REQ-452`, `REQ-469`, `REQ-470`, `REQ-471`, `REQ-472`, `REQ-473`, `REQ-474`, `REQ-475`, `REQ-476`, `REQ-477`, `REQ-478`, `REQ-479`, `REQ-480`, `REQ-481`, `REQ-482`, `REQ-483`, `REQ-484`, `REQ-485`, `REQ-486`, `REQ-487`, `REQ-488`, `REQ-489`, `REQ-490`, `REQ-491`, `REQ-492`, `REQ-493`, `REQ-494`, `REQ-495`, `REQ-496`, `REQ-497`, `REQ-498`, `REQ-499`, `REQ-500`, `REQ-501`, `REQ-502`, `REQ-503`, `REQ-504`, `REQ-505`, `REQ-506`, `REQ-507`, `REQ-508`, `REQ-509`, `REQ-510`, `REQ-511`, `REQ-512`, `REQ-513`, `REQ-514`, `REQ-515`, `REQ-516`, `REQ-517`
- **Produces**: `artifact:relay` — devmesh-relay
- **Gates**: cargo test -p devmesh-relay
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` PostgreSQL、账号和项目设备。
  - **Test and RED**: 在 devmesh-relay/src/{auth,store,catalog}.rs；migrations/；tests/api.rs 对应测试中新增以下行为：SQL 迁移；bootstrap、用户/ACL、Key/设备、Gitea 仓库映射；测试凭据不等于真实环境验证。运行 `cargo test -p devmesh-relay`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: SQL 迁移；bootstrap、用户/ACL、Key/设备、Gitea 仓库映射；测试凭据不等于真实环境验证。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-relay/src/{auth,store,catalog}.rs；migrations/；tests/api.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-relay`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:relay` 增加“PostgreSQL、账号和项目设备”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` WSS 采集与 REST/SSE 查询。
  - **Test and RED**: 在 devmesh-relay/src/{connection,events,query,feed}.rs；tests/transport.rs 对应测试中新增以下行为：注册归属、连续 ACK、幂等事件；事务写投影/feed；快照游标续订；Catalog publisher 与版本冲突。运行 `cargo test -p devmesh-relay`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 注册归属、连续 ACK、幂等事件；事务写投影/feed；快照游标续订；Catalog publisher 与版本冲突。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-relay/src/{connection,events,query,feed}.rs；tests/transport.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-relay`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:relay` 增加“WSS 采集与 REST/SSE 查询”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T03` 远程指令与真实回执。
  - **Test and RED**: 在 devmesh-relay/src/commands.rs；tests/commands.rs 对应测试中新增以下行为：命令持久化后路由当前设备；TTL/权限/能力校验；回执历史与unknown；Comment 独立存储。运行 `cargo test -p devmesh-relay`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 命令持久化后路由当前设备；TTL/权限/能力校验；回执历史与unknown；Comment 独立存储。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-relay/src/commands.rs；tests/commands.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-relay`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:relay` 增加“远程指令与真实回执”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## Evidence 2026-11-25T13:15:02.797783+00:00

RED: 缺 Relay 核心/HTTP 方法；GREEN: 账号/ACL、连续ACK与乱序状态投影、归属、Catalog、命令、HTTP Router 和 Agent↔Relay 真帧双向流测试通过。PostgreSQL migrations 与窄 psql Adapter 已实现并编译；环境同时禁止端口和 shmat，因此真实 PostgreSQL/HTTP网络 Gate 留最终环境，不将 MemoryStore 测试作为数据库验收。实现差异和未完成发布门槛将保留在交接报告。

## 验收审计

内存 Repository/HTTP router/帧与状态测试通过；PostgreSQL 未执行；邀请/审计/角色细分及设备推送尚未完成。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
