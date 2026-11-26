# Todo: 复用本地 PostgreSQL 17 镜像

- **Status**: 验证成功
- **Scope**: Compose 使用用户已有的 postgres:17，明确 pull_policy=missing。

## Behavior / Test first

存在本地 postgres:17 时复用；缺失时才拉取。先用实际 Compose config JSON 检查 image 和 pull_policy，旧配置应失败。只调整 DevMesh PostgreSQL 镜像选择，不迁移已有数据库。

## Verification

`python3 scripts/test_deploy.py`，真实 Compose 配置解析及 Make dry-run。当前执行环境无法访问 Colima socket，不声称实际容器启动通过。

## Done

- [x] RED 已观察。
- [x] 配置修复与测试通过。

## Evidence

RED: 实际 Compose 输出 image=postgres:17-bookworm。GREEN: 修复后 3 项发布测试通过，实际 Compose 输出 postgres:17 / pull_policy=missing；Make dry-run 通过。Colima 实际启动留待用户重试。
