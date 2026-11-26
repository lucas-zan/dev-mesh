# Todo: 一键发布与本地人工安装

- **Status**: 验证成功
- **Scope**: Relay Make/Compose 发布；本地 CLI、Skill/MCP 与 Pi Extension 一键安装；提供人工验收步骤。

## Behavior

- `make -C devmesh-relay` / `make local` 生成并保留部署环境文件，构建 Web+Relay、启动 PostgreSQL、检查健康、首次初始化账号；重复发布不重置账号或数据库。（后续远程发布任务将显式 `make deploy` 改为 SSH 发布入口。）
- `make install` 构建独立版本的本机包并安装三 Runtime/官方 MCP；将 Skill 注册到个人 Codex，使用官方 CLI 注册 MCP/Pi Package，保留其他配置。
- 配置示例随包交付；缺少项目凭据时仅安装，不自动启动 Worker 或执行 Issue。
- 当前环境只验证命令编排、Compose 配置、隔离配置注册和安装文件，不声称 Docker/在线安装已实际通过。

## Test first

- 发布：命令顺序、首次账号、重复发布、健康失败；Make dry-run；真实 compose config。
- 安装：独立版本、Skill 链接、Codex MCP 参数、Pi 本地包注册、重复注册和保留用户文件；隔离 HOME 范围内验证。
- RED: 无 Relay Makefile/发布管理器；无本地宿主安装入口，包中缺配置示例。

## Verification

Python 聚焦测试、Rust/TS 相关检查、隔离发布包安装、文档一致性与新发布包。

## Done

- [x] 部署入口和测试完成。
- [x] 安装入口和测试完成。
- [x] 人工测试说明与发布包完成。

## Evidence 2026-11-26T02:06:34.689881+00:00

RED: 缺少 deploy/manage.py、install_local.py；Codex 注册缺 env_vars。GREEN: 部署 2 项、安装 3 项、实际解包入口安装/回滚/卸载 1 项通过；真实 make install（offline、隔离 prefix/profile）、实际 Codex 配置注册、Compose 配置解析通过。

未实际执行 Docker up 或上游联网安装；由用户按 manual-install-test.md 人工验收。
