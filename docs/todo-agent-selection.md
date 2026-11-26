# Todo: 安装时检测、选择并复用本机 Agent

- **Status**: 验证成功
- **Scope**: make install 的 Agent 选择和安装行为。

## Behavior

- 检测 PATH 和 DevMesh 自有目录中的 Codex/Pi 可执行程序及版本；Pi Durable 按 Host 实际需要的三个 SDK 包检测（项目、npm 全局、自有目录），不把 node 存在视为安装完成。
- 交互安装：只发现一个时显示详情并要求确认，拒绝或空输入后进入选择菜单；多个时列出并支持序号多选；未安装项明确标记，只有用户选中才下载。零个时必须选择安装项。
- 非交互：显式 --runtime 可重现；auto 无论检测数量均明确要求选择。离线非交互默认保持仅安装包行为。
- 复用现有程序/SDK 通过自有目录链接接入现有 Worker 启动路径，不覆盖外部安装；仅下载选中但缺失的 Agent。
- 配置引导的 Runtime 仅从本次选中项选择；只选一个时直接使用。只有选中 Codex 才注册 Codex MCP/Skill；Pi 适配由 Worker 加载。
- 保留已有项目配置；已有项目依赖未选中的 Runtime 时不移除它，并明确显示所保留的项目设置。

## Test first

- Python 检测：多个程序、无程序、失败 --version、Durable SDK（不是 CLI）与路径。
- 选择：单个确认后复用、多选、未安装项、非交互歧义、离线禁止下载。
- 实际复用链接：可执行假 Agent 启动；Durable 三包链接；重复安装幂等。
- Rust CLI --runtime 支持 none/逗号列表，保留 all 并拒绝非法名称；先测试再实现。
- 配置引导支持仅一个选择，已有普通引导回归不变。

## Implementation

- scripts/agent_selection.py + test_agent_selection.py：检测/选择/复用边界。
- install_local.py 串联；package.py 带入检测脚本；setup_local.py 限定选择。
- CLI runtime_names 只决定本轮需要下载哪些 Agent；官方 Gitea MCP 行为保持。
- 不修改 Runtime 协议或全局 Pi 配置，不下载未选择的 Agent。

## Verification

- Python 选择/引导/注册测试、Rust CLI 单测、隔离实际发布包安装与现有启动路径验证、相关文档一致性。
- 不访问真实服务、用户 HOME 或安装上游依赖。

## Done

- [x] RED。
- [x] 实现与文档。
- [x] 聚焦检查和隔离安装通过，限制记录。

## Evidence

- RED：缺少 agent_selection 模块与 Rust runtime_names 函数；配置引导缺少 runtimes 参数；workspace 依赖跨 node_modules 层级时检测失败，补齐实际 Node 模块路径解析后通过。
- GREEN：Agent 选择 7 项、配置引导 11 项、安装/注册 5 项、隔离发布包安装 1 项、Rust CLI 选择 1 项，共 25 项通过。
- 实际启动复用的可执行文件；复用本机真实缓存 Pi Durable SDK 后，复制包内 Host 启动并发出 hello；无需网络或模型调用。
- 实际发布包在线安装分支使用假的 Go/npm 边界，证明 --runtime none 仍调用 Gitea MCP 安装且不调用 npm；安装/回滚/卸载通过。
- Rust fmt 与 Clippy all-targets -D warnings、Python 编译检查通过。
- 限制：未下载上游 Agent、未实际写个人宿主/钥匙串，未声称本机模型开发流程已经验收。
