# DM-04: 本地 Worker、Worktree 与 Bridge

- **Todo ID**: DM-04
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
本地 Worker、Worktree 与 Bridge；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-008`, `REQ-009`, `REQ-010`, `REQ-107`, `REQ-108`, `REQ-109`, `REQ-110`, `REQ-111`, `REQ-112`, `REQ-125`, `REQ-126`, `REQ-127`, `REQ-128`, `REQ-129`, `REQ-130`, `REQ-131`, `REQ-132`, `REQ-133`, `REQ-134`, `REQ-135`, `REQ-136`, `REQ-137`, `REQ-138`, `REQ-139`, `REQ-140`, `REQ-141`, `REQ-142`, `REQ-143`, `REQ-144`, `REQ-145`, `REQ-146`, `REQ-147`, `REQ-148`, `REQ-149`, `REQ-150`, `REQ-151`, `REQ-152`, `REQ-153`, `REQ-154`, `REQ-155`, `REQ-156`, `REQ-157`, `REQ-158`, `REQ-159`, `REQ-160`, `REQ-161`, `REQ-162`, `REQ-163`, `REQ-164`, `REQ-165`, `REQ-166`, `REQ-167`, `REQ-168`, `REQ-169`, `REQ-170`, `REQ-171`, `REQ-172`, `REQ-173`, `REQ-247`, `REQ-248`, `REQ-249`, `REQ-250`, `REQ-251`, `REQ-252`, `REQ-253`, `REQ-254`, `REQ-255`, `REQ-256`, `REQ-257`, `REQ-258`, `REQ-259`, `REQ-260`, `REQ-261`, `REQ-262`, `REQ-263`, `REQ-264`, `REQ-265`, `REQ-266`, `REQ-267`, `REQ-268`, `REQ-269`, `REQ-270`, `REQ-271`, `REQ-272`, `REQ-273`, `REQ-274`, `REQ-275`, `REQ-276`, `REQ-277`, `REQ-278`, `REQ-279`, `REQ-280`, `REQ-281`, `REQ-282`, `REQ-410`
- **Produces**: `artifact:worker` — devmesh-agent/src
- **Gates**: cargo test -p devmesh-agent --test worker; cargo test -p devmesh-agent --test bridge; cargo test -p devmesh-agent --test delivery
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` 本地执行状态、并发和 Worktree。
  - **Test and RED**: 在 devmesh-agent/src/{worker,workspace,lifecycle}.rs；tests/worker.rs 对应测试中新增以下行为：预留槽再 CAS，固定基线，专用 worktree，不改人工目录；等待审批/输入占槽，终态静止后释放。运行 `cargo test -p devmesh-agent --test worker`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 预留槽再 CAS，固定基线，专用 worktree，不改人工目录；等待审批/输入占槽，终态静止后释放。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/{worker,workspace,lifecycle}.rs；tests/worker.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test worker`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:worker` 增加“本地执行状态、并发和 Worktree”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` 事件、控制队列和持久化恢复。
  - **Test and RED**: 在 devmesh-agent/src/{storage,collector,controller,bridge}.rs；tests/bridge.rs 对应测试中新增以下行为：消息先脱敏再 SQLite；sequence 连续；连续 ACK 后清理；过期/旧实例/终态指令拒绝；unknown 不重放。运行 `cargo test -p devmesh-agent --test bridge`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 消息先脱敏再 SQLite；sequence 连续；连续 ACK 后清理；过期/旧实例/终态指令拒绝；unknown 不重放。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/{storage,collector,controller,bridge}.rs；tests/bridge.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test bridge`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:worker` 增加“事件、控制队列和持久化恢复”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T03` 按角色验证真实交付证据。
  - **Test and RED**: 在 devmesh-agent/src/delivery.rs；tests/delivery.rs 对应测试中新增以下行为：PR 候选 SHA、不可变产物、AC 测试和 QA 对象绑定；review 释放，merge 后 reconcile；缺证据 blocked。运行 `cargo test -p devmesh-agent --test delivery`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: PR 候选 SHA、不可变产物、AC 测试和 QA 对象绑定；review 释放，merge 后 reconcile；缺证据 blocked。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/delivery.rs；tests/delivery.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test delivery`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:worker` 增加“按角色验证真实交付证据”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## Evidence 2026-11-25T12:34:00.550093+00:00

RED: 缺 storage/workspace/controller/collector/delivery 模块；GREEN: 7 个聚焦行为测试通过，包括真实临时 Git Worktree、SQLite 重启、终态/旧 epoch 拒绝和证据 SHA/AC 绑定。Worker prepare/running/finalize 为供 Runtime 消费的本地确定性接口；真实网络 Gate 延后。

## 验收审计

本地 Worker/Git/队列/恢复测试通过；真实进程组静止与完整 QA/reconcile 证据关联仍待完成。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
