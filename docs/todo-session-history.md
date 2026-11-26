# Todo: MCP 历史 Session 读取

- **Status**: 验证成功
- **Scope**: DM-03 session_list/get/events，遵循 V1.1 Relay 只读历史合同。

## Behavior

- Input: session_list 的 work_id 或 issue_ref、cursor、limit；session_get/events 的 session_id；events 可带 after_sequence。
- Output: Relay 的授权 Session 快照或分页事件，保留分页游标；不把本地 Work 伪装成 Session。
- Constraints/errors: 没有 Relay 明确 not_configured；只发送 GET；Relay 权限错误原样返回；绑定 Work 不限制读取 Relay 已授权的历史。

## Test first

- MCP 注入只读 HistoryReader，证明三个工具均路由 Relay、分页参数保留，拒绝未配置或缺少 session_id。
- Relay Router 查询按 work_id/issue_ref 筛选，排除无权项目。
- Expected RED: 当前 Context 无 history 边界，session_* 路由本地 Work；Relay 无列表路由。

## Implementation

增加窄 HistoryReader/HTTP GET Adapter；CLI 与受管 MCP 注入配置，凭据按读取时解析。Relay 添加分页 GET /sessions；共享工具及 OpenAPI 同步生成。

## Verification

- `cargo test -p devmesh-agent --test history --test mcp --offline`
- `cargo test -p devmesh-relay --test http --offline`
- API 合同检查、Rust fmt/Clippy、相关回归。

## Done

- [x] RED 已观察。
- [x] 聚焦/回归通过并记录证据。

## Evidence 2026-11-26

- RED: HistoryReader 模块缺失；GET /sessions 返回 404。
- GREEN: history 3 项、MCP 3 项、HTTP 授权筛选测试通过；API 合同检查及 Clippy 通过。
- Changed files: Agent history.rs、mcp.rs、cli.rs、daemon.rs、session.rs；Relay http.rs；共享工具/OpenAPI 生成脚本及对应测试
- Limitation: 真实网络/浏览器验收尚未执行；不表示 DM-03/09 整节点完成。
