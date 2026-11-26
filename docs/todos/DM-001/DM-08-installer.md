# DM-08: 三 Runtime 一键安装与 CLI

- **Todo ID**: DM-08
- **Execution Map**: execution-map.json
- **Todo maturity**: DECOMPOSED
- **Task detail format**: v1
- **Source baseline**: traceability.json source_documents (SHA-256)

## Outcome
三 Runtime 一键安装与 CLI；按固定源合同交付可验证行为。

## Contract
- **Requirements**: `REQ-017`, `REQ-018`, `REQ-028`, `REQ-029`, `REQ-030`, `REQ-031`, `REQ-032`, `REQ-033`, `REQ-034`, `REQ-035`, `REQ-036`, `REQ-037`, `REQ-038`, `REQ-039`, `REQ-040`, `REQ-041`, `REQ-042`, `REQ-043`, `REQ-044`, `REQ-045`, `REQ-046`, `REQ-047`, `REQ-372`, `REQ-373`, `REQ-374`, `REQ-375`, `REQ-376`, `REQ-377`, `REQ-378`, `REQ-379`, `REQ-380`
- **Produces**: `artifact:installer` — devmesh-agent/src/installer.rs
- **Gates**: cargo test -p devmesh-agent --test installer
- **Boundaries**: App 延后；真实凭据仅 DM-12 使用；仅适用当前功能场景，不扩展通用加固矩阵。

## Tasks
- [ ] `T01` 统一 install/init/doctor。
  - **Test and RED**: 在 devmesh-agent/src/{cli,installer,config}.rs；tests/installer.rs 对应测试中新增以下行为：检测 OS/runtime 版本；安装本工具包、Skill、官方 MCP 与三适配；合并配置，doctor 运行实际握手。运行 `cargo test -p devmesh-agent --test installer`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: 检测 OS/runtime 版本；安装本工具包、Skill、官方 MCP 与三适配；合并配置，doctor 运行实际握手。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/{cli,installer,config}.rs；tests/installer.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test installer`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:installer` 增加“统一 install/init/doctor”；节点 Gate 通过后由 execution-map.json 中的后继消费。
- [ ] `T02` 升级、回滚和卸载。
  - **Test and RED**: 在 devmesh-agent/src/installer.rs；scripts/install.sh；tests/installer.rs 对应测试中新增以下行为：manifest 只记录自有文件；校验发布哈希、备份用户配置、失败回滚；卸载保留用户数据。运行 `cargo test -p devmesh-agent --test installer`；实现前应因缺失该行为或模块失败，保存输出。
  - **Execution logic**: manifest 只记录自有文件；校验发布哈希、备份用户配置、失败回滚；卸载保留用户数据。边界解析后调用具体领域函数；失败返回明确错误，不猜测式降级。
  - **Files and responsibilities**: devmesh-agent/src/installer.rs；scripts/install.sh；tests/installer.rs；入口负责解析，领域模块负责状态，Adapter 负责外部 I/O。
  - **Verification**: `cargo test -p devmesh-agent --test installer`；验证成功路径、合同规定的非法状态/重复提交/失败路径；涉及 CAS/槽位时验证并发，纯文档检查引用；不涉及迁移回滚的任务无需人工矩阵。
  - **Artifact handoff**: 向 `artifact:installer` 增加“升级、回滚和卸载”；节点 Gate 通过后由 execution-map.json 中的后继消费。

## Tests
测试命令及用例见每个任务的 Verification；外部集成使用隔离测试服务，真实环境验收单独记录。

## Done
- [ ] 任务及节点 Gate 通过，实际证据写入本文件和 execution-map.json。
- [ ] 图谱、机器追溯校验通过，独立重读源文档确认语义覆盖。

## STOP conditions
源合同/摘要漂移先更新追溯；未知外部副作用不重放；缺实际上游能力不得声称通过；缺凭据只阻塞最终真实环境验收。

## Evidence 2026-11-25T12:51:57.030993+00:00

RED: 无安装器；GREEN: 2 个安装校验和/升级回滚/用户配置与文件保留测试通过，CLI 帮助和无凭据 MCP 初始化 2 测试通过。统一入口支持三 Runtime 固定版本 npm 安装；当前沙箱不能联网，下载式安装与 doctor 原生能力认证留 DM-12；发布制品在 DM-11 构建。

## 验收审计

隔离离线安装/更新/回滚/卸载通过；三 Runtime 和官方 MCP 在线安装、目标机器握手未验收。 已有 Evidence 仅证明其中明确记录的本地检查，不表示本节点全部合同完成。完整差距见 `../../acceptance-status.md`。
