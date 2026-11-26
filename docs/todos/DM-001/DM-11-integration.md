# DM-11: 跨项目离线集成与发布制品

- **Todo ID**: DM-11
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
跨项目离线集成与发布制品；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-024`, `REQ-025`, `REQ-381`, `REQ-382`, `REQ-383`, `REQ-384`, `REQ-385`, `REQ-386`, `REQ-387`, `REQ-388`, `REQ-389`, `REQ-390`, `REQ-391`, `REQ-406`, `REQ-407`, `REQ-409`
- **Produces**: `artifact:integration` — scripts
- **Gates**: python3 scripts/test_e2e.py; python3 scripts/verify.py
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` 真实本地服务集成。
  - **Test and RED**: 在 scripts/test_e2e.py；各仓 integration tests 对应测试中新增以下行为：测试 Gitea HTTP 服务与受管假 Runtime 连通 Agent/Relay/Postgres/Web；20 Worker 同抢、断线重传、旧指令拒绝。运行 `python3 scripts/test_e2e.py`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 测试 Gitea HTTP 服务与受管假 Runtime 连通 Agent/Relay/Postgres/Web；20 Worker 同抢、断线重传、旧指令拒绝。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: scripts/test_e2e.py；各仓 integration tests；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `python3 scripts/test_e2e.py`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:integration` 增加“真实本地服务集成”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` 构建部署与全新目录安装。
  - **Test and RED**: 在 Makefile；devmesh-relay/deploy/；scripts/verify.py；README.md 对应测试中新增以下行为：Rust/TS lint/test/build、Compose 部署、安装 tar/哈希；全新临时 HOME install/doctor/update/uninstall，文档明确验证范围。运行 `python3 scripts/verify.py`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: Rust/TS lint/test/build、Compose 部署、安装 tar/哈希；全新临时 HOME install/doctor/update/uninstall，文档明确验证范围。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: Makefile；devmesh-relay/deploy/；scripts/verify.py；README.md；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `python3 scripts/verify.py`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:integration` 增加“构建部署与全新目录安装”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## 验收审计

make verify 全部本地检查通过，隔离发布包安装通过；真实端口/PostgreSQL/Compose/浏览器联调未执行。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
