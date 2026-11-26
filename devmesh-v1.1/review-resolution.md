# V1.1 设计评审闭环与开发验收

本轮依据用户提供的评审截图中可见的 P0/P1 项及五处实现风险，逐项修订原 V1.0。截图底部第 5 项后续文字未完整显示，因此不声称覆盖未提供的具体意见。本表中的决策已写入主规格，避免仅在补充说明中另设一套逻辑。

| 优先级 / 评审项 | V1.1 决策 | 对应章节 |
|---|---|---|
| P0 Claim 生命周期 | 执行静止后 CAS 状态+释放；未知保留；Assignee 专供人工，旧机器指派管理员核对清理；返工新 generation/Work | 03、04、05 |
| P0 Issue/Work/Session 联动 | 顶层轮次完成与 Work 终态区分；idle grace 内 Prompt；终态不可控；本地审批/输入占槽、超时与收尾 | 06.4、08.2–08.3、09 |
| P0 Gitea 写入归属 | 官方 MCP 只读 + 受控写 facade；Claim/交付 Worker 强制校验，禁止受管模型绕过 | 04.5、07 |
| P0 公共协议 | ID、事件、命令、回执、IPC、状态、错误及兼容规则；主要 JSON Schema 与正反例 fixtures | protocol/contract.md 与 schemas |
| P0 Runtime Adapter | Rust/TS NDJSON、原生生命周期映射、实例/epoch、退出/取消/恢复、能力证据与审批 | 06 |
| P1 Catalog 同步 | 每项目一个发布 lease，完整快照/增量/epoch，源版本冲突回读，关闭/缺失/删除区分 | 08.7、10 |
| P1 交付与依赖 | 四角色交付证据、不可变 artifact/commit、候选 SHA 审核、返工及实际 merge/close 校验 | 05.4–05.5 |
| P1 账号与项目初始化 | 管理员 bootstrap、邀请登录、Gitea 身份/仓库映射、ACL、Key、设备/App 配对和撤权 | 02、10.1 |
| 完整消息 vs 源码 | 允许共享的完整语义消息；默认源码和代码块省略，字段脱敏/截断、缺口标记；Spool 无原始敏感内容 | 08.4–08.6、11、12 |
| Issue 元数据 | 完整版本化结构、标签和权威字段；ready/active claim 约束、依赖及交付字段；样例与 Schema | 03、protocol/schemas/issue.schema.json |

## 追加发布测试（T11–T24）

| ID | 测试 | 通过标准 |
|---|---|---|
| T11 | 机器认领、review、rework 再领 | A/B 各自 Claim generation 正确；机器从不修改人工 assignees；未指派任务可由 B 再领，人工指派 A 的任务不误分给 B |
| T12 | 失败/取消/掉线/接管 | 确认静止后释放；进程未知、Gitea 断开或旧设备可能继续运行则保持 Claim；无双执行 |
| T13 | 轮次完成及 Prompt | idle grace 内可新轮次；Work finalizing/terminal 时 Prompt/Steer/Follow-up 被拒绝；Comment 不触发 Agent；旧代次指令不可注入 |
| T14 | 等待审批/输入 | 三 Runtime 真实请求到本机 CLI；保留槽；确认、拒绝、超时、取消均有原生回执和最终状态；无远程审批接口 |
| T15 | Gitea 写入隔离 | 受管模型不能调用 raw write/读 PAT/直接推送；所有受管交付操作验证 Claim，撤销后拒绝；普通 Gitea 权限未被扩大 |
| T16 | 结构及协议兼容 | Rust/TS/Server 使用同一 Schema；正常 fixture 通过，非法字段/缺 epoch/Claim 拒绝；不兼容版本拒绝，不静默降级 |
| T17 | 原生状态/IPC 崩溃 | stdout 不混日志；子进程退出不伪造终态；未知副作用不重放；同 Work 恢复，实例改变，旧指令拒绝 |
| T18 | 脱敏/源码/截断 | 代码块、秘密、路径、隐藏推理、私人会话不上传；原文不进入 Spool；截断标识、允许记录完整、缺口显式 |
| T19 | Catalog 多设备 | 旧 lease epoch、过期批次拒绝；快照缺页不删除；同更新时间不同 hash 诊断/回读；离线 stale；closed 与 missing/deleted 区分 |
| T20 | 交付证据与版本 | QA 对 SHA H1 的通过不能用于 H2；artifact 使用不可变引用；CI/审核不足不得合并关闭；非代码任务也需证据 |
| T21 | 初始账户及仓库映射 | 首次管理员 bootstrap 一次；邀请过期、伪造设备、项目未授权、Key 撤销均拒绝；同 repo 改名不重复创建 |
| T22（后续） | App 关联和撤权；不纳入本期发布门槛 | 一次性配对码不能重放；App 无 ingest Key；用户退出或撤权后 WSS/SSE/控制立即失效 |
| T23 | 断线 ACK/命令不确定 | 未持久化不 ACK；已确认事件无重复展示；控制优先，超时不重新派发；回执丢失标 unknown |
| T24 | 本机槽位与任务异常 | 本机全局/项目并发不超限；审批等待占槽；Release CAS 失败进入对账，不能重复启动；Relay 故障不影响 Gitea 正常任务 |

## 已执行与待执行

本包的 3 个 JSON Schema 采用 Draft 2020-12，并执行 13 个正例及 4 个反例的结构校验，均符合预期。样例中的 UUID、Runtime 版本、地址、事件及回执均为合成测试数据。该检查不等于 Rust/TS 代码编译、真实 Gitea CAS、上游 Runtime/本地审批、权限或性能验收。D0 仍需公司实际 Gitea 版本、三 Runtime 固定版本与可运行环境；D1 要从此合同冻结完整 OpenAPI/AsyncAPI 文件及生成 SDK、实现跨字段状态测试。未通过门槛不得将自动认领或某 Runtime 标记为可正式使用。
