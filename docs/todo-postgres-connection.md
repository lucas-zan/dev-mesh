# Todo: 修复 Relay PostgreSQL URL 连接

- **Status**: 验证成功
- **Scope**: PostgreSQL 命令适配边界。

## Behavior

- 输入：DATABASE_URL 完整连接 URL；SQL 经 stdin 传递。
- 输出：psql 使用 --dbname 显式解析 URL；成功返回 stdout，失败返回 psql 具体 stderr。
- 保留原有迁移和持久化，不修改账号、密码或数据库卷。

## Test first

- 正常：命令将 URL 传到 libpq 的显式 dbname 参数，查询成功返回 stdout。
- 错误：非零状态保留数据库错误详情，而非吞掉 stderr。
- RED：新增命令/输出边界测试，旧实现缺少对应行为。
- 复现：真实 psql 在 PGDATABASE=postgresql://...@devmesh-db.invalid 时连接 /tmp socket；显式 --dbname 后尝试解析指定主机。

## Implementation

- storage.rs 提取 psql 命令构造与输出解析，模块内对应单测。
- 不涉及数据库重建、依赖替换或重试机制。

## Verification

- cargo test -p devmesh-relay --offline；Rust fmt/clippy。
- 当前环境不能访问 Colima 或实际测试数据库，不宣称容器修复已实测。

## Done

- [x] RED 已观察。
- [x] 连接与错误详情修复。
- [x] 聚焦回归通过。

## Evidence

- RED：新增 3 项测试因缺少命令/输出处理边界而编译失败。
- GREEN：cargo test -p devmesh-relay --offline：22 passed，1 个真实 PostgreSQL 测试因无专用数据库而 ignored；clippy --all-targets -D warnings 通过；部署 Python 8 项通过。
- 真实 psql 对比证明 PGDATABASE URL 会尝试 /tmp socket；--dbname URL 则解析给定主机。
- 已修复连接参数，并保留 psql 错误详情；尚未在用户 Colima 中重建验证启动。不删除数据库、不重置配置。
