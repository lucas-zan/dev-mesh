# DevMesh

DevMesh 把 Gitea 的 Issue、受管的本地编码 Runtime 与远程工作台连接起来。Gitea 是任务与交付事实源；Agent 在本机认领任务、创建独立 Git worktree、运行模型和执行受控交付；Relay 保存工作记录、同步项目视图并转发远程命令。

**服务名称统一为 `devmesh-relay`。App 延后开发，待其他功能验证后直接对接 Relay REST/SSE。当前版本是待真实环境验收的开发版本。**

第一期以功能验证和流程走通为范围，见[第一期合同](docs/phase1-functional.md)与[真实验收步骤](docs/live-validation.md)；邀请、完整审计和生产安全加固不作为本期完成条件。

| 目录 | 内容 | 技术 |
| --- | --- | --- |
| `devmesh-agent` | Skill、Gitea MCP、Worker、Bridge、CLI、安装器、Codex/Pi Adapter | Rust；Skill 为 Markdown |
| `devmesh-agent/packages/pi-extension` | Pi 原生 RPC Extension、受控工具 | TypeScript |
| `devmesh-agent/packages/pi-durable-host` | Pi Durable SDK Host、持久会话和工具 | TypeScript |
| `devmesh-protocol` | 公共 Schema、Rust/TS 类型、REST 客户端、OpenAPI | Rust、TypeScript、JSON |
| `devmesh-relay` | 账号、设备、项目、工作记录、REST/WSS/SSE、PostgreSQL 边界 | Rust |
| `devmesh-relay/web` | Web 工作台、管理入口 | React、TypeScript |
| `devmesh-v1.1` | 设计基线 | Markdown、JSON |

依赖顺序：公共协议 → 本地 Skill/Gitea → Worker 与三 Runtime → CLI/安装器 → Relay → Web。运行时由 Agent 主动连接 Relay；模型的 Gitea 写入经过本地受控 MCP，远程命令经过当前 Work/Session/Runtime 实例校验。

## 本地开发

需要 Rust 1.89+、Node.js 24、npm、Git。实际三 Runtime 和官方 Gitea MCP 的安装还需要网络，官方 MCP 的安装使用 Go。

```sh
npm ci --ignore-scripts
make build
make verify
python3 scripts/test_e2e.py
```

`make verify` 执行格式、Clippy、Rust/TS 测试、Web 构建、API/Skill 一致性和隔离安装测试；结果写入 [latest.json](docs/verification/latest.json)。离线环境的 Cargo 依赖必须预先缓存。`test_e2e.py` 使用真实子进程管道、Git、SQLite 和内存传输，不代表真实网络或模型验收。

## 一键安装

在项目根目录一键构建、安装并注册个人 Codex Skill/MCP：

```sh
make install
```

分发给同事时，请发送与对方系统和 CPU 架构匹配的发布包。他们在解压后的包目录运行同一条 `make install` 即可进入安装引导。

先检测本机 Codex、Pi 与 Pi Durable：仅一个可用时显示名称、版本和路径，经用户确认后复用；多个时由用户选择，支持多选。只下载选中但缺失的 Agent，并安装官方 Gitea MCP；仅选中 Codex 时注册其 Skill/MCP，保留其他配置。交互终端中会逐步引导配置 Gitea、Relay 和默认工作目录，自动注册设备、保存系统凭据、配置 PATH 并检查连接，无需手工编辑 JSON 或导出 Token。Pi/Durable 适配由 Worker 自动加载。完整安装及人工测试步骤见[安装测试说明](docs/manual-install-test.md)。`--offline` 跳过上游依赖下载；安装成功与原生模型验收是不同阶段。

安装前缀默认是 `~/.local/share/devmesh`，命令位于该目录的 `bin/devmesh`。可以用 `--prefix DIR` 指定隔离目录。更新、回滚、卸载命令见 `devmesh --help`；卸载保留配置和用户数据。

## 配置与运行 Agent

安装时已引导配置；已经安装但未配置时，在源码根目录运行：

```sh
make configure
```

引导保留已有配置，凭据使用 macOS Keychain 或 Linux Secret Service。Token 可访问的所有仓库自动发现，无需输入 owner/name 或逐个接入。高级配置可参考 [配置示例](examples/config.json)；`projects` 仅用于单仓库运行参数覆盖，不限制发现范围。

首次安装时选择本机能接的角色，支持逗号分隔的多选，例如 `backend,qa`。之后运行 `devmesh configure` 修改角色。正在运行的 Worker 在重启后读取新配置。角色决定接单资格，Issue 的查看范围由 Gitea Token 决定。

交互安装结束时会询问是否启动 Worker，回车默认启动并等待就绪，输入 `n` 可跳过。首次配置默认关闭自动认领。选中 Codex 时，安装完成后在新 Codex 会话用 `$devmesh` 查看仓库或指定一个 Issue 开发。远程控制默认关闭，原生审批和输入在本机处理。

定时自动执行时，在明确授权的项目覆盖中开启 `auto_claim`。已获无人值守和 Full Access 授权的 Codex 项目还可以设置 `codex_unattended: true`，让 Worker 用 Full Access 启动新会话和恢复会话。修改后重启 Worker。已认领的任务仍通过 DevMesh 工具测试、推送和提交交付记录。[Worker 自动执行](devmesh-agent/skills/devmesh/references/worker-automation.md)

## 运行 Relay 与 Web

本地验证需要 Docker Compose v2、Python 3、Make，在项目根目录执行：

```sh
make -C devmesh-relay
```

自动构建 Web/Relay、启动 PostgreSQL，并在首次发布时创建管理员。访问 `http://机器地址:17843`，使用 `make -C devmesh-relay credentials` 查看初始凭据。重复执行同一命令即可更新，详见 [Relay 发布说明](devmesh-relay/README.md)。

发布到远程时，在本地执行 `make deploy REMOTE_HOST=user@server`。本地按远程架构构建最终镜像，通过 SSH 上传并启动，远程无需 Rust、Node 或完整源码；远程需 Docker Compose v2、Python 3 和可写的部署目录，默认 `/opt/devmesh`，可通过 `REMOTE_DIR` 调整。

下面是直接运行二进制的替代方式：

Relay 的普通启动需要 `DATABASE_URL` 和可执行的 `psql`。首次创建账号：

```sh
target/debug/devmesh-relay migrate
# DEVMESH_INITIAL_PASSWORD 由本机环境提供；bootstrap 仅首次执行。
target/debug/devmesh-relay bootstrap --username admin
target/debug/devmesh-relay serve --web-dir devmesh-relay/web/dist
```

`bootstrap` 返回的一次性设备 Key 属于秘密，不提交或粘贴到日志。Web 使用初始账号密码登录。容器发布使用前述 Make 入口；数据库密码和首次账号配置保存在 `devmesh-relay/deploy/.env`。

本地临时演示可运行 `devmesh-relay serve --ephemeral --web-dir devmesh-relay/web/dist`，需 `DEVMESH_INITIAL_PASSWORD`，退出即丢失数据。Web 默认地址为 `http://127.0.0.1:17843`。

## 设计、任务和验收

- [V1.1 设计](devmesh-v1.1/DevMesh-V1.1.md)、[疑问与设计决议](devmesh-v1.1/review-resolution.md)
- [开发拆解](docs/implementation-plan.md)、[执行图](docs/todos/DM-001/execution-map.json)
- [当前验收状态与实现差异](docs/acceptance-status.md)
- [REST 合同](devmesh-protocol/openapi/devmesh.json)、[本地 Skill](devmesh-agent/skills/devmesh/SKILL.md)

尚未通过真实环境验收的项目保留待验收状态；不据离线测试声明 V1.1 全部完成。
# dev-mesh
