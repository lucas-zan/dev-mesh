# Todo: 设备离线状态与实时通知

- **Status**: 验证成功
- **Scope**: DM-09/10 设备在线状态；依赖现有已测 Repository/feed 接口，不改变账号权限模型。

## Behavior

- Input: 设备连接、断开或管理员撤销设备/Key。
- Output: 保存 online/last_seen；状态变化时向设备已有 Work 的项目发出 device.changed。
- Constraints/errors: 心跳不重复发布状态变化；撤销连接时也必须置为离线；只向有项目 read 权限的读者返回 feed。

## Test first

- Relay 测试 online→offline 产生可见 feed，重复 offline 不重复 feed。
- HTTP 撤销已在线设备/Key 后查询保存的状态必须为 false。
- Expected RED: 当前没有状态 feed，撤销后保存的 online 仍为 true。

## Implementation

沿用事务 commit 同时保存设备和项目 feed；撤销路径显式更新设备状态，再通知连接停止。不增加后台循环。

## Verification

- `cargo test -p devmesh-relay --test transport --test http --offline`
- Relay 回归、Rust fmt/Clippy。

## Done

- [x] RED 已观察。
- [x] 聚焦/回归通过并记录证据。

## Evidence 2026-11-26

- RED: 离线无 feed；HTTP 撤销设备后 online 仍为 true。
- GREEN: transport 4 项及 HTTP 聚焦/相关测试通过；Clippy 通过。
- Changed files: devmesh-relay/src/events.rs、http.rs；tests/transport.rs、http.rs；Web main.tsx（现有离线控制测试映射）
- Limitation: 真实网络/浏览器验收尚未执行；不表示 DM-03/09 整节点完成。
