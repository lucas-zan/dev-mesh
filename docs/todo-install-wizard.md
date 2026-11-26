# Todo: 本地安装配置引导

- **Status**: 验证成功
- **Scope**: 本地安装后配置与检查的一条引导流程。

## Behavior

- 交互终端 make install 安装后自动引导；make configure 可单独运行。--configure/--no-configure 支持显式选择，非交互默认不提问。
- 引导输入 Gitea URL/Token、owner/repo、Relay URL/管理员密码、本地路径、Runtime/角色/测试命令。
- Gitea API 获取 numeric ID；Relay 登录后选择已有项目或创建项目，自动创建设备和 Key。macOS Keychain/Linux Secret Service 保存凭据，配置仅引用凭据名。
- 写正式 config.json，现有配置原样保留。Worker enabled 但不启动，auto_claim=false；缺少仓库时启用 auto_clone。
- 自动更新 shell PATH，自动 doctor --verify；模型登录仍由 Runtime 原生流程处理，检查结果如实显示。
- 失败明确退出；不连接未经用户输入的真实服务，不覆盖数据库或已有用户配置，不写 Token 到 shell 文件。

## Test first

- HTTP/系统凭据/命令边界使用替身，验证真实配置可用字段、项目复用、自动设备注册、凭据不进 JSON、doctor 调用。
- 已有配置不提问、不写入、不创建设备；API 错误不写正式配置；PATH 幂等并覆盖含空格前缀。
- 首先观察缺少 setup_local.py 的 RED。

## Implementation

- scripts/setup_local.py 负责引导与 API/凭据边界；test_setup_local.py 对应测试。
- install_local.py 接入，package.py 随包交付引导；Makefile 提供 configure；文档统一。
- 不增加守护进程或自动开发逻辑，不增加通用配置框架。

## Verification

- 引导与安装相关 Python 测试；实际 CLI 配置解析（隔离目录）；发布包安装回归；Make dry-run。
- 不执行用户 HOME 写入、真实网络注册或系统钥匙串写入；真实人工安装待用户验证。

## Done

- [x] RED 已观察。
- [x] 引导与安装/包入口一致。
- [x] 聚焦验证通过，文档更新。

## Evidence

- RED：test_setup_local.py 因缺少 setup_local 模块失败；追加测试发现 Gitea MCP 必须使用安装目录绝对路径，修复后通过；凭据提供方错误测试证明不输出 Token。
- GREEN：引导 9 项（含实际 DevMesh CLI 接受生成配置）、安装注册 3 项（含实际 Codex CLI）、隔离发布包安装/回滚/卸载 1 项通过；Skill 一致性 3 项、Python 编译检查与 Make configure dry-run 通过。
- 包内交付 setup_local.py，源码/解压包共用流程；未访问真实服务或写入用户 HOME/钥匙串。
- 自动连接检查会展示 Runtime 原生协议状态，模型账号登录和真实开发流程仍需本机验证。
