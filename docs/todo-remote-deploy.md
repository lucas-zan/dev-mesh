# Todo: 本地构建并通过 SSH 发布 Relay/Web

- **Status**: 验证成功
- **Scope**: 一条本地构建、镜像传输、远程启动的发布流程。

## Behavior

- 输入：SSH host、远程绝对目录（默认 /opt/devmesh）、可选镜像名和平台。
- 输出：按远程架构本地 buildx 构建；save/scp/load；远程 Compose 启动并复用已有 bootstrap。
- 约束：远程需 Docker Compose v2/Python3，SSH 用户需目录与 Docker 权限；无镜像仓库。远程不构建源码，不覆盖已有 .env/数据库。失败立即停止，不连接未配置的机器。
- 兼容：默认 make 与 make local 保留本地发布；显式 make deploy 改为远程发布，根 Makefile 同样提供入口。

## Test first

- 正常：检测架构→本地 build→传输→远程 load/start；runtime Compose 没有 build。
- 边界：ARM64/x86_64 映射；缺 host、不支持架构明确失败；路径带空格正确传递。
- 错误：构建失败不上传，不启动远程；重复启动保留已有环境与账号。
- RED：远程模块与 deploy-image 行为尚不存在。

## Implementation

- scripts/test_remote_deploy.py 对应 deploy/remote.py；test_deploy.py 覆盖 manage.py 的 image-only 启动与环境保留。
- Compose 拆分运行配置与本地 build override；远程只上传运行配置和管理脚本。
- Make 与 README 更新命令合同；不执行真实远程发布（尚未指定远程主机）。

## Verification

- 聚焦 Python 行为测试、实际 Compose config、Make dry-run。

## Done

- [x] 观察 RED。
- [x] 实现与文档完成。
- [x] 聚焦回归通过，说明真实远程验证限制。

## Evidence

- RED：远程测试因 remote.py 尚不存在失败；manage 测试因缺少 image 参数及运行配置包含 build 失败。
- GREEN：test_deploy.py 4 项、test_remote_deploy.py 4 项通过；真实 Compose CLI 解析 runtime 与 build override 配置通过；Python 编译检查、根目录与 Relay 目录 Make dry-run 通过。
- 缺少 REMOTE_HOST 时入口明确失败，未连接任何主机。
- 限制：命令边界使用测试替身，未执行真实 Docker build/save、SSH/SCP 或远程启动。尚未指定远程地址；当前环境不能访问 Colima socket。用户报告的本地 Relay 退出仍需其容器日志才能诊断，未宣称修复。
