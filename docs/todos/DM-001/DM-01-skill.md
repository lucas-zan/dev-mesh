# DM-01: 本地协作 Skill

- **Todo ID**: DM-01
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
本地协作 Skill；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-001`, `REQ-002`, `REQ-212`, `REQ-213`, `REQ-214`, `REQ-405`
- **Produces**: `artifact:skill` — devmesh-agent/skills/devmesh/SKILL.md
- **Gates**: python3 scripts/test_skill.py
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [x] `T01` 角色化协作与跨仓需求。
  - **Test and RED**: 在 devmesh-agent/skills/devmesh/；scripts/test_skill.py 对应测试中新增以下行为：根据角色读取规范，调用受控 MCP 创建/评论/报告；模型不直接修改 Claim 或决定交付完成。运行 `python3 scripts/test_skill.py`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 根据角色读取规范，调用受控 MCP 创建/评论/报告；模型不直接修改 Claim 或决定交付完成。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/skills/devmesh/；scripts/test_skill.py；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `python3 scripts/test_skill.py`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:skill` 增加“角色化协作与跨仓需求”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [x] `T02` 需求、缺陷、测试模板与交付示例。
  - **Test and RED**: 在 devmesh-agent/skills/devmesh/references/ 与 templates/；scripts/test_skill.py 对应测试中新增以下行为：模板携带验收项、执行角色、依赖和证据；明确 blocked/rework 的操作入口。运行 `python3 scripts/test_skill.py`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 模板携带验收项、执行角色、依赖和证据；明确 blocked/rework 的操作入口。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/skills/devmesh/references/ 与 templates/；scripts/test_skill.py；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `python3 scripts/test_skill.py`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:skill` 增加“需求、缺陷、测试模板与交付示例”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [x] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [x] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## Evidence 2026-11-25T12:18:55.352800+00:00

RED: 缺 Skill/角色文件导致 FileNotFoundError；GREEN: 3 个分发一致性检查通过。Skill 为指令与模板，不伪造运行测试。四角色、跨仓依赖、返工和交付均有明确调用路径。
