# 本地一键安装与人工测试

## 一键安装

安装需要 Python 3、Make、Node.js 24+、Git、Go 和网络。下载缺失的 Agent 时需要 npm。从源码安装还需要 Rust 1.89+。在交互终端中，安装器会引导配置 Gitea 和 Relay。模型账号仍按各 Agent 的方式登录。

在完整项目根目录执行：

```sh
make install
```

该命令先检测本机 Agent，再自动构建本机包并安装：

- 只检测到一个可用 Agent：显示名称、版本和路径，询问是否复用；输入 y/yes 才确认，拒绝或直接回车进入选择菜单。
- 检测到多个：列出版本与位置，由用户输入序号选择，支持逗号多选。
- 未安装的 Agent 会标记为“选中后下载”；没有可用 Agent 时，选择需要安装的一项或多项。
- Codex/Pi 检测可执行程序，Pi Durable 检测 Host 所需的 SDK 依赖（包括 workspace 的 Node 模块解析路径），不是只检测 Node。
- 已有 Agent 通过 DevMesh 自有目录的链接复用，不覆盖全局安装。只下载本次选中但缺失的 Agent。

| 组件 | 安装/加载方式 |
| --- | --- |
| DevMesh CLI/Worker | `~/.local/share/devmesh/bin/devmesh` |
| Codex、Pi、Pi Durable | 复用已检测的选中项；缺失且选中时才安装固定版本到自有目录 |
| 官方 Gitea MCP | 自动安装到 DevMesh 自有目录 |
| Codex Skill | 选中 Codex 时，注册 `~/.agents/skills/devmesh` |
| Codex MCP | 选中 Codex 时，通过 Codex CLI 注册到个人 `config.toml`，保留其他配置 |
| Pi Extension | 随包安装，由 Worker 启动 Pi 时自动加载 |
| Pi Durable Host | 随包安装，由 Worker 启动 Durable 时自动加载 |
| 配置引导 | 自动生成 `~/.local/share/devmesh/config.json`；示例也随包保留 |
| 凭据 | macOS Keychain / Linux Secret Service，配置中只保存引用 |
| PATH | 自动加入 zsh/bash 配置，新终端生效 |

不需要分别复制 Skill/插件，也不需要发布到插件市场。普通 Pi 的全局配置不改动；Pi/Durable 的人工流程测试通过 DevMesh Worker 发起。

Codex 的个人 Skill 目录与符号链接遵循[官方 Skill 文档](https://learn.chatgpt.com/docs/build-skills#where-codex-loads-local-skills)。安装器为 MCP 配置凭据环境变量名转发；STDIO 环境转发参数见[官方 MCP 文档](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)。

在解压后的发布包目录运行以下命令。安装器直接使用包里的二进制，DevMesh 无需在本机编译。

```sh
make install
```

不再默认安装全部 Agent。只有选中 Codex 才注册其个人 Skill/MCP；Pi/Durable 的适配由 Worker 加载，现有其他宿主配置保持不变。Worker-only 安装可加 `--skip-hosts`。显式选择可用于非交互脚本：

```sh
make install INSTALL_ARGS='--runtime codex'     # 已有则复用，缺失才安装
make install INSTALL_ARGS='--runtime codex,pi'  # 选择多个
make install INSTALL_ARGS='--runtime all'       # 显式选择全部
```

每次源码安装生成独立版本，选择结果记录在安装前缀的 `agents.json`，已有配置保留；旧版单仓库配置会增加 Token 全仓库发现设置，并保留原有路径、凭据和设备。非交互 auto 模式要求显式传 `--runtime`，即使只检测到一个 Agent 也不会自动确认。`--offline` 禁止下载，非交互且未显式选 Agent 时保持仅安装组件文件；显式选择缺失 Agent 会报错。

## 安装时直接完成配置

在普通交互终端执行 `make install`，安装完成后会依次询问：

1. Gitea 地址（默认 `http://git.meditrust.xyz`，回车使用，也可修改）和 Token。Token 隐藏输入，授权范围为其可见的全部仓库。
2. Relay 地址（默认 `http://127.0.0.1:17843`）、管理员用户名与密码，自动注册设备。
3. 所有仓库共用的工作目录；选中多个 Agent 时选择默认 Runtime。
4. 本机接单角色。从 `backend`、`frontend`、`product`、`qa` 中选择，可输入 `backend,qa` 多选或输入 `all` 全选。

不询问 owner/name、仓库 ID、项目接入或每个仓库的测试命令。Skill 的 `repository_list` 自动分页发现 Token 可见的公开、私有及协作仓库；后续操作用仓库 ID 定位。读写操作是否有权执行由 Gitea 判断。开始开发时，Worker 上传 Work 和 Session 及其仓库、Issue 标识。Relay 工作台只展示有会话的 Issue，按所属项目分组。

Git 的 HTTP(S) 克隆、拉取和推送使用同一个 Gitea Token。已有仓库显式配置的 SSH 地址继续使用本机 SSH 凭据。

设备 ID 与设备 Key 由 Relay 自动创建，不需要手动复制。Token 和 Key 保存到系统凭据存储，macOS 可能弹出钥匙串授权；Linux 需要可用的 `secret-tool` / Secret Service。无需手动 `export` 凭据，也无需手动编辑 JSON。

引导写入正式配置，自动执行 `doctor --verify` 并展示结果；这是连接和协议检查，不能代替实际模型开发验收。模型账号仍需按 Codex/Pi 原生方式登录。安装结束时会询问是否启动 Worker，直接回车会启动并检查就绪状态。输入 `n` 可跳过，需要时执行 `devmesh worker start`。新发现仓库默认 `auto_claim=false`。执行时按需克隆到统一工作目录下的 `owner/name`；模型读取仓库 AGENTS.md，使用 `work_run_test(command=实际测试命令)` 运行并记录测试证据。已有显式测试配置仍通过 index 使用。

安装完成后打开新终端使 PATH 生效；选中 Codex 时，在其新会话使用 `$devmesh`，若未出现则重启 Codex。已有凭据和设备保留，不重复注册；旧版配置自动启用全仓库发现。

已经安装但尚未配置时，无需重装：

```sh
make configure
# 自定义安装前缀时：
make configure CONFIGURE_ARGS='--prefix /absolute/path/to/devmesh'
```

解压包也支持 `python3 setup_local.py --prefix /absolute/path/to/devmesh`。

已经安装的用户运行 `devmesh configure` 修改角色，直接回车保留当前选择。若 Worker 正在运行，等当前任务结束后重启即可应用。手动接单和自动认领都检查 Issue 的 `required_role`。查看 Issue 仍按 Gitea Token 权限。

非交互安装或 `--offline` 默认跳过配置和 Worker 启动。自动化安装已有配置时可传 `--start-worker`。也可以显式控制：

```sh
make install INSTALL_ARGS='--configure'     # 强制进入引导
make install INSTALL_ARGS='--no-configure'  # 暂不配置
make install INSTALL_ARGS='--no-start-worker'  # 跳过启动 Worker
```

现有环境变量/系统凭据引用与 `config.example.json` 仍可用于高级手动配置；不再是普通安装的必需步骤。

## 3. 人工验证完整流程

在 Codex 中输入：

> $devmesh 列出 Gitea Token 可访问的所有仓库，查看待处理 Issue，先不要启动开发。

确认 Gitea 读取成功后，选择一个专用测试仓库创建小任务，让 Skill 创建结构化 Issue，授权准备为 ready（所需标签自动初始化）。已有 manual-only Issue 在用户明确授权 Agent 执行后，可使用 `devmesh issue authorize --repository-id 仓库ID --number Issue编号` 写入授权，再准备为 ready；该命令不会自动开始开发。若安装时跳过启动，先执行 `devmesh worker start`。然后执行：

```sh
devmesh repos
devmesh worker status
devmesh work --repository-id 仓库ID --number 测试Issue编号
```

观察 `state_dir/daemon.log` 和 Web：任务被认领、Runtime 启动、实际改动/测试、专属分支和 PR 出现、交付进入 review。人工合并后，Worker 下一轮确认 done/close。`starting` 输出不等于启动成功，必须以 status、日志和实际记录确认。

依次将 `repositories.runtime` 改为已选中的 `codex`、`pi`、`pi-durable` 测试；切换前先 `devmesh worker stop`，更新配置后再启动。更完整的 QA、控制和恢复步骤见 [真实环境验收](live-validation.md)。

最后按需设置 `repositories.auto_claim=true`，重启 Worker，再创建一个 ready Issue，验证无需手动 work 命令即可被周期拉取、认领并开发。

## 4. 本次已验证与待人工验证

已验证：Make 编排、真实 Compose 配置解析、隔离目录中的实际安装、实际 Codex CLI 配置注册、用户其他配置保留、Skill 链接、交互流程的 API/凭据替身测试、正式配置被实际 CLI 接受及随包引导入口。系统钥匙串写入和真实联网引导仍待人工验证。

当前执行环境不能监听 TCP/Unix socket，且不能联网安装上游 Runtime，因此没有声称 Docker 服务部署、联网安装或真实模型任务已通过。这些正是本次人工测试要执行的内容。
