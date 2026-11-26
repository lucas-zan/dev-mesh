# Todo: 以 Token 权限发现所有仓库，移除人工项目接入

- **Status**: 验证成功
- **Scope**: Token 所授权仓库作为安装、Skill/MCP、Worker 与 Relay 目录的共同范围。

## Behavior

- 安装只配置 Gitea 地址/Token、Relay、Agent 和工作目录；不输入 owner/name、仓库 ID，不要求逐仓库配置。
- Gitea /repos/search?private=true 分页发现 Token 可见的全部仓库，包括协作/组织仓库；服务端继续决定具体操作权限。
- 配置保存 discovery 的实例 ID、workspace root、默认 Runtime；projects 仅为兼容旧配置和明确覆盖，不作为 discovery 白名单。
- MCP 列出全部可见仓库，跨仓库读写按 repository_id 定位；多仓库时不默选首个仓库执行操作。目录每次调用更新；零仓库也可启动并返回空列表。
- Worker 每轮刷新仓库；显式工作使用 repository_id，避免列表索引随刷新变化。自动认领默认关闭，保留 Claim/Work 调用归属。
- Relay 内部项目记录只是仓库投影，自动同步，无人工接入步骤；非管理员 Key 无权自动创建时明确报错，不能阻止本地仓库读取。
- 空测试默认允许 Work 内按仓库 AGENTS.md 传入实际测试命令并记录既有证据，不给不同语言仓库硬编码同一测试命令。
- 旧手工配置保持兼容；不删除数据库、外部安装或已有项目。实际权限由 Gitea API 判定，不新增权限体系。

## Test first

- 分页、去重、权限报错、空目录；新仓库与权限移除在下次发现生效。
- discovery 配置投影、默认路径/Runtime、旧覆盖、仓库选择缺失明确报错。
- MCP 两仓库读写路由、刷新、空目录；Work 使用显式 repository_id；Relay 自动同步无需人工注册。
- 安装无 owner/name/测试命令提示，Token 验证和系统存储保持；实例复用和重复运行不创建设备。
- 先观察上述缺失行为的 RED；实际网络用 GiteaTransport/HTTP 边界替身，不伪称真实部署验收。

## Implementation

- discovery.rs 负责窄仓库发现/投影；config.rs 定义默认配置并提供解析。
- CLI/MCP/daemon/Relay bridge 消费解析后的仓库目录；tools.json 更新明确目标和仓库测试命令参数。
- setup_local.py 仅账户与默认运行配置，测试与文档同步；沿用既有 Runtime/Claim/交付协议。

## Verification

- Rust 聚焦 discovery/MCP/session/daemon 相关测试与全 workspace 回归、fmt/clippy；Python 配置与安装/包回归；TS/Skill/API 合同检查。
- 不访问真实 Gitea/用户 HOME 或系统凭据，不启动真实模型开发。

## Done

- [x] RED。
- [x] 安装、MCP、Worker、Relay 共用 Token 仓库范围。
- [x] 聚焦和相关回归通过，真实环境限制如实记录。

外部 API 合同：使用 [Gitea Repository Search](https://docs.gitea.com/api/operations/repo-search/) 包含可访问私有仓库；不设置 owner/uid 限制，避免只枚举自有仓库。

开发路径补充验收：在没有本机 Git 凭据时，克隆、拉取和推送私有 HTTP(S) 仓库复用已配置的 Gitea Token。客户端仅对同源地址设置进程级 Git 凭据。普通 Issue 查看不要求 DevMesh 元数据，认领和交付仍使用既有 Work 合同。

## Evidence

- 安装引导测试覆盖零仓库安装和旧配置迁移。Rust 测试覆盖分页、权限变化、普通 Issue 查看、Work 路由及 Relay 仓库映射。
- 本地授权 HTTP 服务要求 Token。真实 Git 进程克隆成功，仓库配置只记录 URL。另有 Work 流程测试实际运行测试命令并推送到本地 Git 仓库。
- `scripts/verify.py` 通过全部检查，见 [验证记录](verification/latest.json)。线上 Gitea、Relay 和模型联调未执行。
