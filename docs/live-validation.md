# 第一期真实环境验收

范围见 [第一期功能合同](phase1-functional.md)。真实 Gitea/模型、网络、PostgreSQL 和浏览器验收必须在允许这些操作的机器上运行。当前沙箱的结果见 `verification/live.json`，环境检查失败、doctor 未执行，不能当成验收通过。

## 1. 安装与配置

在专用测试仓库验收。安装解压后的平台包：

```sh
sh /absolute/path/devmesh-0.1.0/install.sh --source /absolute/path/devmesh-0.1.0
```

默认安装三 Runtime 和官方 Gitea MCP，需要 Node/npm、Git、Go 与网络。`--offline` 只验证 DevMesh 文件安装，不代表上游 Runtime 安装成功。先运行 `devmesh doctor`。

从 `examples/config.json` 创建本机配置，填写 Gitea/Relay 地址、仓库 ID、实例 UUID、设备 ID、实际测试命令和 Runtime。Token/模型登录仅放本机环境或凭据引用。确认目标 Gitea 的 Issue 响应提供本项目使用的 content_version CAS；没有此能力不能把认领验证记为通过。

先用单项目配置验证一个 Runtime，设置 `worker.enabled=true`、`auto_claim=false`、`auto_clone=true`，使用专用 state_dir。为 Codex、Pi、Pi Durable 分别运行同一流程；切换配置时先停止旧 Worker 或使用独立 state_dir。初始化角色/状态标签：

```sh
devmesh init --from /absolute/path/config.json
devmesh issue init
python3 scripts/validate_live.py --config /absolute/path/config.json
```

该入口检查本机监听/进程检查能力、真实 Gitea/Relay 身份和配置 Runtime 的原生握手。它不会启动模型任务，不能替代下面的验收。

## 2. 实际开发与交付

在专用仓库创建一个小任务，例如实现一个确定输入输出的函数并补测试。通过 DevMesh Skill 创建带 AC 的 Issue，再由配置允许的准备入口进入 ready。

```sh
python3 scripts/validate_live.py --config /absolute/path/config.json --run-issue 128 --project 0 --timeout 600 --output docs/verification/live-codex-task.json
```

`--run-issue` 明确授权启动 Worker 并执行该 ready Issue。脚本只接受专用配置中所有项目 auto_claim=false，避免同次单任务验收混入其他自动任务。它等待实际交付进入 review/done；blocked/rework 或超时均失败。后台 Worker 保留运行，以便观察和合并后收尾。

检查实际仓库改动、测试输出、候选 SHA、任务分支、PR 和 delivery，而非仅看模型回复。代码任务应释放 Claim 后留在 review。人工在 Gitea 审核并合并，Worker 下一轮应将 canonical state 改为 done 并关闭 Issue；如 PR 已自动关闭 Issue，不应重复 close。也可显式执行：

```sh
devmesh issue reconcile --number 128
```

## 3. QA 与周期调度

创建独立 QA Issue，通过 pr_ready 指向生产任务/PR。QA 的工作区和测试证据应固定在候选 SHA；测试后可以把报告提交到独立版本，artifact 引用报告 commit，candidate_sha 仍为被测版本。人工验收：

```sh
devmesh issue accept --number 129
```

完成单任务验证后启用项目 `auto_claim=true`，设置所需 `poll_interval_seconds` 并重启 Worker。创建第二个 ready Issue，验证它在下一次轮询被发现、认领和执行。暂停应只阻止新认领；已有任务继续，已交付任务仍可确认合并。

## 4. Relay/Web 与原生控制

启动 PostgreSQL + Relay，建立与 Gitea 仓库对应的项目/设备 Key，浏览器登录 Web。分别确认：Issue 出现；当前/历史 Session 记录、工具与回执更新；长消息/工具展开/加载更早记录可用；设备断开后显示离线。

对足够长的测试任务验证 busy Steer、follow-up、空闲 Prompt、中断、原生本地审批/输入。观察真实回执；accepted 不等于 applied。不支持的能力应明确拒绝。审批在 Agent 本机完成。

最后验证 Worker 正常 stop、重启后的状态和显式恢复；不确定旧实例是否退出时应保留 recovery_required，不能重复执行外部操作。

`validate_live.py` 的报告明确列出未覆盖的浏览器、原生控制/恢复和人工合并步骤；这些步骤的实际观察需另行记录。全部实际检查完成前，不将第一期验收记为通过。
