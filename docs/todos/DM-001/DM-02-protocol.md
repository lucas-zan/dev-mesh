# DM-02: 公共协议与领域类型

- **Todo ID**: DM-02
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
公共协议与领域类型；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-003`, `REQ-004`, `REQ-048`, `REQ-049`, `REQ-050`, `REQ-051`, `REQ-052`, `REQ-053`, `REQ-054`, `REQ-055`, `REQ-056`, `REQ-057`, `REQ-058`, `REQ-059`, `REQ-060`, `REQ-072`, `REQ-073`, `REQ-074`, `REQ-075`, `REQ-076`, `REQ-077`, `REQ-078`, `REQ-079`, `REQ-233`, `REQ-234`, `REQ-235`, `REQ-236`, `REQ-237`, `REQ-238`, `REQ-239`, `REQ-240`, `REQ-241`, `REQ-242`, `REQ-243`, `REQ-244`, `REQ-245`, `REQ-246`, `REQ-412`, `REQ-413`, `REQ-414`, `REQ-415`, `REQ-416`, `REQ-417`, `REQ-418`, `REQ-419`, `REQ-420`, `REQ-421`, `REQ-422`, `REQ-423`, `REQ-424`, `REQ-425`, `REQ-426`, `REQ-427`, `REQ-428`, `REQ-429`, `REQ-430`, `REQ-431`, `REQ-432`, `REQ-433`, `REQ-434`, `REQ-435`, `REQ-436`, `REQ-437`, `REQ-438`, `REQ-439`, `REQ-440`, `REQ-441`, `REQ-442`, `REQ-443`, `REQ-444`, `REQ-445`, `REQ-446`, `REQ-447`, `REQ-448`, `REQ-449`, `REQ-450`, `REQ-451`, `REQ-452`, `REQ-453`, `REQ-454`, `REQ-455`, `REQ-456`, `REQ-457`, `REQ-458`, `REQ-459`, `REQ-460`, `REQ-461`, `REQ-462`, `REQ-463`, `REQ-464`, `REQ-465`, `REQ-466`, `REQ-467`, `REQ-468`, `REQ-503`, `REQ-504`, `REQ-505`, `REQ-506`, `REQ-507`, `REQ-508`, `REQ-509`, `REQ-510`, `REQ-511`, `REQ-512`, `REQ-513`, `REQ-514`, `REQ-515`, `REQ-516`, `REQ-517`
- **Produces**: `artifact:protocol` — devmesh-protocol
- **Gates**: cargo test -p devmesh-protocol
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` 冻结协议结构与状态机。
  - **Test and RED**: 在 devmesh-protocol/{src,schemas,fixtures,ts}/；tests/contracts.rs 对应测试中新增以下行为：复用 V1.1 schema；Rust/TS 共享 UUID、字符串序号、Issue/Work/Session 和命令状态；非法转换返回显式错误。运行 `cargo test -p devmesh-protocol`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 复用 V1.1 schema；Rust/TS 共享 UUID、字符串序号、Issue/Work/Session 和命令状态；非法转换返回显式错误。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-protocol/{src,schemas,fixtures,ts}/；tests/contracts.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-protocol`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:protocol` 增加“冻结协议结构与状态机”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` REST/WSS/SSE 合同与幂等身份。
  - **Test and RED**: 在 devmesh-protocol/{openapi,asyncapi}/；tests/contracts.rs 对应测试中新增以下行为：OpenAPI/AsyncAPI 明确注册快照与事件增量；同 frame 重试幂等，新实例注册是新 frame，不覆盖历史实例。运行 `cargo test -p devmesh-protocol`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: OpenAPI/AsyncAPI 明确注册快照与事件增量；同 frame 重试幂等，新实例注册是新 frame，不覆盖历史实例。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-protocol/{openapi,asyncapi}/；tests/contracts.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-protocol`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:protocol` 增加“REST/WSS/SSE 合同与幂等身份”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## Evidence 2026-11-25T12:23:11.537458+00:00

RED: unresolved imports for missing public behaviors；GREEN: 3 Rust 合同测试与 17 个 AJV fixtures 通过。生成 TS 声明，保留完整设计合同。OpenAPI 当前仅已精确定义控制与 SSE，其他端点在 Relay 节点补齐，未声称完整 SDK 已交付。

## 验收审计

公共 Schema、类型、SDK 与三 Adapter 帧合同测试通过；Rust 嵌套边界校验仍未完全覆盖源合同。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
