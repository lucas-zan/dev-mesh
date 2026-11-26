# Relay/Web 发布

## 本地构建，上传到远程运行

在本地项目根目录执行（也支持 `make -C devmesh-relay deploy`）：

```sh
make deploy REMOTE_HOST=user@server
```

`REMOTE_HOST` 也支持已配置的 SSH 别名。SSH 登录和 Docker 权限需事先可用。脚本自动检测远程 ARM64/AMD64，在本地构建匹配架构的最终镜像，通过 SSH/SCP 上传并 `docker load`，然后启动 PostgreSQL 与 Relay/Web。远程不下载 Rust/Node 构建环境，也不需要完整源码或镜像仓库。Web 已包含在 Relay 镜像中。

本地需要 Docker Buildx（跨架构构建需支持模拟执行）、Python 3、Make、SSH 和 SCP（OpenSSH 9+ 默认 SFTP）；远程需要 Linux、Docker、Compose v2、Python 3，以及 SSH 用户可写的部署目录。默认远程目录 `/opt/devmesh`，普通用户可改为自己的绝对路径：

```sh
make deploy REMOTE_HOST=my-server REMOTE_DIR=/home/deploy/devmesh
# 自动识别架构；必要时显式指定
make deploy REMOTE_HOST=my-server PLATFORM=linux/amd64
```

默认镜像标签带 UTC 时间；可通过 `RELAY_IMAGE=devmesh-relay:v1` 自定义。每次发布只上传最终 Relay 镜像、Compose 运行配置和管理脚本。镜像以完整 tar 上传，每次传输完整镜像，不提供镜像仓库的分层增量传输；首次远程启动还会拉取独立的 `postgres:17` 镜像。远程执行 `up --no-build`，复用已有环境文件、账号和数据库卷，不删除历史镜像。

首次生成远程 `.env` 和管理员账号，终端输出初始用户名与密码；设备 Key 保存在远程 `.local/bootstrap.json`。访问 **http://远程机器地址:17843**。以下命令在远程部署目录执行：

```sh
python3 manage.py status
python3 manage.py logs
python3 manage.py credentials
python3 manage.py restart
python3 manage.py down  # 保留数据库卷
```

远程 `.env` 可调整 `RELAY_PORT` 和 `RELAY_BIND_ADDRESS`。修改配置后再次在本地 `make deploy` 生效；初始密码不会重置已有账号。脚本会记录本次 `RELAY_IMAGE`，保留其余配置。

## 本地功能验证

原有本地构建和启动命令保留，默认目标仍为本地：

```sh
make -C devmesh-relay
# 或 make -C devmesh-relay local / make relay-local
```

需要 Docker Compose v2、Python 3 和 Make；Rust、Node 在 Docker 内构建。首次生成 `deploy/.env`，启动 PostgreSQL 与 Relay/Web，健康检查通过后创建初始账号。访问 http://127.0.0.1:17843；默认绑定 `0.0.0.0`。

```sh
make -C devmesh-relay credentials
make -C devmesh-relay status
make -C devmesh-relay logs
make -C devmesh-relay restart
make -C devmesh-relay down
make -C devmesh-relay config
```

这些 Make 管理命令针对本地服务。`ENV_FILE=/absolute/path/.env` 可指定本地环境文件。PostgreSQL 使用 `postgres:17`，优先复用同标签本地镜像；本地构建所需的 Node/Rust/Debian 基础镜像首次仍需下载。

若 Relay 启动退出，用 `make -C devmesh-relay logs` 查看具体错误。数据库连接通过 `psql --dbname DATABASE_URL` 指定服务地址；完整连接 URL 不能仅通过 `PGDATABASE` 环境变量传递，否则可能尝试连接容器自身的 Unix socket。更新此修复后重新执行本地构建命令即可，不需要删除数据库卷或重新生成密码。
