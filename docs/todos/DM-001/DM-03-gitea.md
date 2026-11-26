# DM-03: Gitea 受控本地 MCP

- **Todo ID**: DM-03
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
Gitea 受控本地 MCP；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-005`, `REQ-006`, `REQ-007`, `REQ-061`, `REQ-062`, `REQ-063`, `REQ-064`, `REQ-065`, `REQ-066`, `REQ-067`, `REQ-068`, `REQ-069`, `REQ-070`, `REQ-071`, `REQ-080`, `REQ-081`, `REQ-082`, `REQ-083`, `REQ-084`, `REQ-085`, `REQ-086`, `REQ-087`, `REQ-088`, `REQ-089`, `REQ-090`, `REQ-091`, `REQ-092`, `REQ-093`, `REQ-094`, `REQ-095`, `REQ-096`, `REQ-097`, `REQ-098`, `REQ-099`, `REQ-100`, `REQ-101`, `REQ-102`, `REQ-103`, `REQ-104`, `REQ-105`, `REQ-106`, `REQ-113`, `REQ-114`, `REQ-115`, `REQ-116`, `REQ-117`, `REQ-118`, `REQ-119`, `REQ-120`, `REQ-121`, `REQ-122`, `REQ-123`, `REQ-124`, `REQ-215`, `REQ-216`, `REQ-217`, `REQ-218`, `REQ-219`, `REQ-220`, `REQ-221`, `REQ-222`, `REQ-223`, `REQ-224`, `REQ-225`, `REQ-226`, `REQ-227`, `REQ-228`, `REQ-229`, `REQ-230`, `REQ-231`, `REQ-232`
- **Produces**: `artifact:gitea` — devmesh-agent/src/gitea
- **Gates**: cargo test -p devmesh-agent --test gitea; cargo test -p devmesh-agent --test mcp
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` Gitea API 与官方 MCP 边界。
  - **Test and RED**: 在 devmesh-agent/src/gitea/{client,mcp}.rs；tests/gitea.rs 对应测试中新增以下行为：配置用户 endpoint 和凭据引用，真实用户身份/仓库 ID；只读官方 MCP 与窄 CAS API 分离。运行 `cargo test -p devmesh-agent --test gitea`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 配置用户 endpoint 和凭据引用，真实用户身份/仓库 ID；只读官方 MCP 与窄 CAS API 分离。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/gitea/{client,mcp}.rs；tests/gitea.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test gitea`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:gitea` 增加“Gitea API 与官方 MCP 边界”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` 创建、准备、认领、释放及返工。
  - **Test and RED**: 在 devmesh-agent/src/gitea/{issue,claims}.rs；tests/gitea.rs 对应测试中新增以下行为：正文唯一 JSON 块初始化 backlog，标签一致才 ready；CAS 409/未知不启动；不修改 assignee；generation 增长。运行 `cargo test -p devmesh-agent --test gitea`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 正文唯一 JSON 块初始化 backlog，标签一致才 ready；CAS 409/未知不启动；不修改 assignee；generation 增长。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/gitea/{issue,claims}.rs；tests/gitea.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test gitea`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:gitea` 增加“创建、准备、认领、释放及返工”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T03` 本地 stdio MCP 与交付写入。
  - **Test and RED**: 在 devmesh-agent/src/mcp.rs；tests/mcp.rs 对应测试中新增以下行为：JSON-RPC tools/list/call；绑定可信 Work 上下文；证据/评论/PR 经 Worker 校验 Claim 和目标版本后执行。运行 `cargo test -p devmesh-agent --test mcp`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: JSON-RPC tools/list/call；绑定可信 Work 上下文；证据/评论/PR 经 Worker 校验 Claim 和目标版本后执行。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/mcp.rs；tests/mcp.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test mcp`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:gitea` 增加“本地 stdio MCP 与交付写入”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## Evidence 2026-11-25T12:28:56.755367+00:00

RED: 缺失 Gitea/MCP 模块；GREEN: 3 个 Gitea CAS/资格/释放返工测试与 2 个 MCP 协议/绑定测试通过。20 并发任务在窄 Transport 测试实现上验证唯一认领；HTTP 测试因沙箱 bind PermissionDenied 未运行，留最终环境验证。MCP Work 操作通过可信 WorkActions 边界接入后续 Worker。

## 验收审计

CAS/标签/释放、MCP 路由本地测试通过；创建中断修复、历史 Session 查询与实际官方 MCP/Gitea 验收未完成。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
