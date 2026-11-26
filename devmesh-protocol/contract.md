# DevMesh Protocol 1.1 — 实现合同

本文与 JSON Schema、fixtures 共同定义首版跨仓接口。API 路径仍为 `/api/v1` 和 `/ws/v1/connect`，协议版本为 major=1/minor=1。Rust、TypeScript 及移动端从相同 Schema 生成/校验类型。本文所列接口为开发合同，不代表已发布；实际 Gitea/Runtime 的原生接口由 Adapter 的 D0 fixture 固定。

## 1. 基本规则

UUID 使用 RFC 4122 字符串；Gitea repository_id、issue_number、user_id 使用正整数；64 位递增序号使用十进制字符串（避免 JS 数值精度问题，比较时转整数）；时间使用 UTC RFC3339，服务端 received_at、issued_at、expires_at 为权威时间。文本 UTF-8；SHA256 为64位小写十六进制，Git SHA 40/64位。所有 ID 不透明、不可推断授权。用户身份由连接认证提供，客户端无权自报 sender/user。`issue_ref=(instance_id,repository_id,issue_number)`；`project_id` 为 Relay 内部 UUID；`issue_id` 为 Relay 内部 UUID、唯一映射 issue_ref；`work_id` 关联 claim_id 与 generation；`session_id` 关联 work_id、parent_session_id 与原生标识；`runtime_instance_id` 每次宿主启动变更；`run_epoch` 是顶层用户轮次标识。`sequence` 从 1 开始，连续，Session 内单调；`cursor` 为不透明字符串，仅用于 API feed。连接 epoch 与 catalog epoch 分离。

版本握手：客户端发送 supported_versions（例如 ["1.1"]），Relay 选择交集中的最高兼容版本；无交集返回 incompatible_version 并关闭。1.1 为首版严格合同，同一协商版本拒绝未定义字段，保留 extensions 扩展对象；minor 新增可选能力须先协商，旧 peer 不得接收未经协商的新增字段/必需事件。major 不兼容须并行 endpoint 或明确迁移。未知扩展事件只接受 `x.*` 且双方声明能力，否则拒绝；核心事件类型不可私自变更语义。JSON Schema 检查结构；权限、跨字段关系、时钟、状态转换、顺序和哈希等由程序校验，不由 Schema 代替。

## 2. WSS frame

统一 `{version,type,frame_id,sent_at,payload}`，`reply_to` 可选，首帧 version 为客户端最高版本（首版实现仅 1.1）；认证经 TLS Authorization Bearer agent key，服务端认证后再接受 register。单帧≤1MiB，事件≤64KiB，默认每批≤32条，禁止压缩炸弹。登录失败/版本失败/不合法设备关闭；协议错误不泄露密钥。所有写操作使用稳定 frame_id / record ID；重放同 ID 相同 hash 返回已有结果，不同内容 conflict。

| type | 方向 | 必填 payload |
|---|---|---|
| connection.register | C→S | device_id、supported_versions、client_version、resume_sessions[{session_id,acked_sequence}] |
| connection.ack | S→C | connection_id、connection_epoch、selected_version、server_time、heartbeat_seconds |
| connection.heartbeat | 双向 | connection_epoch、nonce、reply（bool） |
| catalog.acquire | C→S | project_id |
| catalog.lease | S→C | project_id、epoch、expires_at、renew_after_seconds |
| catalog.snapshot | C→S | project_id、epoch、snapshot_id、snapshot_seq、page_index、is_last、scope、items[] |
| catalog.delta | C→S | project_id、epoch、snapshot_id、snapshot_seq、page_index=0、is_last=true、scope=delta、items[] |
| catalog.ack | S→C | project_id、epoch、snapshot_id、page_index、committed、conflicts[] |
| catalog.refresh | 双向 | project_id、reason；非 publisher 可请求刷新 |
| work.register | C→S | work_id、project_id、issue_ref、claim_id、claim_generation、role、state、started_at |
| session.register | C→S | session_id、work_id、parent_session_id（null 可）、runtime、runtime_version、adapter_version、runtime_instance_id、native_session_id（null 可）、capabilities、remote_control、state |
| session.status | C→S | session_id、runtime_instance_id、native_session_id、run_epoch、state、remote_control、observed_at；状态是瞬时快照，持久审计仍靠事件 |
| session.events | C→S | session_id、events[]（同一 session/work、按 sequence 排序） |
| events.ack | S→C | session_id、acked_sequence、missing_sequences[]（有界） |
| command.dispatch | S→C | command 对象（含目标、sender、type、message、issued_at/expires_at） |
| command.receipt | C→S | command_id、receipt_seq、state、disposition、reason_code、evidence_ref、at |
| record.ack | S→C | record_type、record_id、result |
| protocol.error | 双向 | request_id、code、message、retryable，可选 related_id |

`catalog.acquire` 同一在线 publisher 续租时保持 epoch；换 publisher 才增加 epoch。work/session 注册、Catalog page、command receipt 按 record ID 独立持久化确认，连接重建后恢复未确认项；事件 ACK 只确认连续序号。Server 校验 project ACL、device/user 归属、work/claim 绑定、Session 归属及状态，拒绝跨租户注入。注册时不可让设备抢占另一设备的 Session，旧 connection_epoch 作废。会话重启重新 register 只允许同 owner/work，runtime_instance 改变，历史 session segment 可追溯；目标旧 instance 的指令一律拒绝。单实例 feed 事务按提交顺序分配 cursor，广播失败仍可从 DB 恢复。

## 3. Event、Command、Catalog、IPC Schema

- `schemas/wire.schema.json`：WSS frame 联合类型、事件 envelope、每种 payload、command、receipt、capabilities、catalog item；允许通过 `$defs` 引用用于 REST/SSE 的对象。
- `schemas/issue.schema.json`：Gitea 正文协议块（初始化 staging 允许 issue_ref=null；进入 ready 之前必须补齐），Claim、依赖与交付证据。
- `schemas/ipc.schema.json`：Rust/TS 本地 NDJSON frame、method 列表、请求及回执。method-specific args 见下表；原生协议版本在 D0 固定。

| IPC method | request args | result |
|---|---|---|
| probe | 无 | version、capabilities、health |
| start | cwd、issue_ref、initial_prompt、runtime_config_ref | native_session_id、instance、run_epoch、state |
| attach / resume | native_session_id、workspace、checkpoint_ref | 同 start；只能同非终态 Work |
| status | 无 | state、run_epoch、native_session_id、quiesced |
| steer / follow_up / prompt | command_id、message | disposition、native_receipt_ref（可空） |
| interrupt / stop | reason | quiesced、outcome；未知时 false |
| resolve_approval | approval_id、decision（approve/deny）、可选 scope | accepted、native_receipt_ref |
| reply_input | input_id、text | accepted、native_receipt_ref |

IPC 不发送 Relay Key/PAT；request_id 只用于一次 RPC，command_id 用于业务幂等。Rust 维护 session route/epoch 并在本地核验权限，TS 适配层再串行验证 busy/epoch。Adapter 原生事件到公共类型的 fixture 需包含 start、完整消息、工具开始/完成、等待、turn 终态、steer 接受/拒绝、停止、恢复；不得用 fake fixture 宣称实际验证。

## 4. REST API / SSE

`/api/v1`：JSON 请求/响应；用户 Web 同源 HttpOnly cookie + CSRF，App Bearer 短期 access token，Agent key 仅授权 ingest/catalog/read scope；不以 URL 携带长效 key。所有写接口记录 request_id，POST commands/keys/pair 必须 Idempotency-Key。分页 `limit` 默认50、最大200；`cursor` 不透明并绑定 filter/ACL；返回 `{items,next_cursor,as_of_cursor}`。快照与 feed cursor 同一个一致读视图。

| Method / Path | 请求及关键返回（JSON） |
|---|---|
| POST /auth/login | {username,password} → Web cookie 或 App {access_token,refresh_token,expires_at}；按 client_kind 选择 |
| POST /auth/refresh /logout | refresh 或当前 cookie；轮换/撤销凭据 |
| POST /auth/pair/start | 已登录用户 {purpose,device_name} → {pair_code,expires_at}；只能指定当前账户 |
| POST /auth/pair/complete | {pair_code,device_public_id} → 相应设备凭据；一次使用，按 purpose 限制 scope |
| GET /me | {user_id,display_name,roles,scopes,project_permissions} |
| GET /projects | project summaries：project_id、repo_ref、name、last_synced_at、stale、running_count |
| GET /projects/{id}/issues | state、role、q、cursor、limit；item 含 issue_ref、title、source/canonical state、activity、active_sessions、catalog_conflict |
| GET /issues/{id} | Issue snapshot、work summaries、permission flags、sync status、as_of_cursor |
| GET /issues/{id}/works | Work summaries + sessions，包含 claim_generation、status、timestamps、delivery refs |
| GET /sessions/{id} | Session snapshot、work、capabilities、instance、run_epoch、remote_control、connection、as_of_cursor |
| GET /sessions/{id}/events | 公共 event[]；按 sequence 查询，返回 next_cursor |
| GET /sessions/{id}/messages | message/tool/plan/command 投影（已脱敏），返回 next_cursor |
| POST /sessions/{id}/commands | {type,target,message,ttl_seconds}；202 {command_id,state,expires_at}；只接受 Runtime 控制类型；Comment 使用独立路由 |
| POST /issues/{id}/comments | {work_id(optional),message} → {comment_id,created_at}，不触发 Runtime |
| GET /sessions/{id}/commands | Command + receipts，返回真实状态 |
| GET /events/stream | 项目过滤/授权范围、Last-Event-ID 或 cursor；SSE feed |
| GET /sessions/{id}/stream | 指定 Session SSE feed，与详情 snapshot cursor 配套 |
| GET /devices | 当前用户或授权管理设备信息；无凭据 |
| GET/PUT /subscriptions | 关注 project/issue/session、阅读位置；按当前用户存储 |
| POST /admin/users /projects /keys | 管理创建、邀请、项目绑定、签发；字段见主文档 §10.1 |
| PUT /admin/projects/{id}/members | {user_id,permissions,allowed_roles}；授权审计 |
| DELETE /admin/keys/{id} /devices/{id} | 立即撤销、断开连接和 SSE 权限 |
| POST /admin/issues/{id}/tombstone | {reason,evidence_ref}，管理员明确确认删除/归档；保留历史 |
| GET /admin/audit | actor、action、resource、outcome、cursor |

SSE `id: <feed_cursor>`、`event: project.changed|catalog.changed|work.changed|session.changed|session.event|command.changed|comment.created|resync.required`、`data: {project_id,issue_ref,work_id?,session_id?,kind,payload,received_at}`。每事件持久化 feed 后再广播；按项目过滤及用户权限检查。首次 snapshot 返回 cursor，从严格大于 cursor 的事件续订；Last-Event-ID 恢复；过期 410 + resync，缓冲溢出主动断开并恢复；撤权立即断流。Web/App 以实际状态和 permission flags 禁用控件，不凭本地颜色判断是否可控。`cursor` 不能作为安全令牌。

## 5. 错误与兼容性

统一 `{request_id,error:{code,message,retryable,details?}}`；details 只允许非敏感诊断。HTTP 状态如下，WSS/IPC 复用 code：

| 状态 | code 示例 | 处理 |
|---|---|---|
| 400 | invalid_request, invalid_schema, invalid_target | 修复输入，不自动重试 |
| 401 | unauthenticated, key_revoked | 重新授权/关闭连接 |
| 403 | permission_denied, remote_control_disabled | 不重试；审计 |
| 404 | not_found | 无权和不存在统一不可见 |
| 409 | claim_conflict, invalid_state, stale_turn, stale_instance, event_conflict, idempotency_conflict, catalog_conflict | 回读；不得盲重试 |
| 410 | cursor_expired, command_expired | 重取快照；过期指令不重发 |
| 413 | payload_too_large | 依策略分片或截断 |
| 422 | unsupported_capability, incompatible_version | 不开放相关功能 |
| 429 | rate_limited, queue_full | Retry-After、退避；指令仍校验 TTL |
| 503 | unavailable, offline, unknown_outcome | 读回状态；不重放未知副作用 |

REST OpenAPI/AsyncAPI 文件在 D1 由本合同和 schema 生成/补全，CI 检查实现、生成类型与 schema/fixture 一致。当前包提供设计级合同与可校验的主要 JSON Schema，不声称已完成全量生产 API SDK、代码或实际联调。

工作台查询可用 `with_sessions=true` 只返回有会话的项目和 Issue。Worker 在 `work.register` 提供 Gitea Issue 标识和必要展示信息；`project_id` 为空时，Relay 按实例与仓库 ID 绑定项目。Session 保存后发布目录更新。
