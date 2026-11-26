# DM-12: 最终真实凭据与上游验收

- **Todo ID**: DM-12
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
最终真实凭据与上游验收；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-026`, `REQ-027`, `REQ-392`, `REQ-393`, `REQ-394`, `REQ-395`, `REQ-396`, `REQ-397`, `REQ-398`, `REQ-399`, `REQ-400`, `REQ-401`, `REQ-402`, `REQ-403`, `REQ-404`, `REQ-407`, `REQ-408`, `REQ-409`, `REQ-410`, `REQ-411`
- **Produces**: `artifact:live` — docs/live-validation.md
- **Gates**: python3 scripts/validate_live.py
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` 真实 Gitea/Relay 身份与并发验收。
  - **Test and RED**: 在 scripts/validate_live.py；docs/live-validation.md 对应测试中新增以下行为：由用户提供实际 URL 和凭据引用后，读真实用户/仓库；在专用测试 Issue 做 CAS 与生命周期；不在代码或日志存秘密。运行 `python3 scripts/validate_live.py`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 由用户提供实际 URL 和凭据引用后，读真实用户/仓库；在专用测试 Issue 做 CAS 与生命周期；不在代码或日志存秘密。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: scripts/validate_live.py；docs/live-validation.md；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `python3 scripts/validate_live.py`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:live` 增加“真实 Gitea/Relay 身份与并发验收”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` 三个 Runtime 真机安装与任务控制。
  - **Test and RED**: 在 scripts/validate_live.py；docs/runtime-compatibility.md 对应测试中新增以下行为：固定版本，真实模型工作、同 Session steer、审批、停止与恢复；逐项记录证据，缺 key 明确未验证。运行 `python3 scripts/validate_live.py`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 固定版本，真实模型工作、同 Session steer、审批、停止与恢复；逐项记录证据，缺 key 明确未验证。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: scripts/validate_live.py；docs/runtime-compatibility.md；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `python3 scripts/validate_live.py`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:live` 增加“三个 Runtime 真机安装与任务控制”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## 验收审计

尚未使用真实凭据或模型执行最终验收。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
