# DevMesh

DevMesh connects Gitea issues, local coding agents, and a browser workspace. Gitea holds the issues, pull requests, and delivery records. The DevMesh Agent runs Codex, Pi, or Pi Durable in a dedicated Git worktree and records test and delivery evidence. DevMesh Relay serves the web workspace and synchronizes devices, work records, and sessions with the Agent.

DevMesh is a development release. See the [acceptance status](docs/acceptance-status.md) for coverage and the [live validation guide](docs/live-validation.md) for tests with real Gitea, Relay, and model accounts.

## Deploy Relay and Web

### Run on your machine

Install Docker with Compose v2, Python 3, and Make. Run these commands from the repository root.

```sh
make -C devmesh-relay
make -C devmesh-relay credentials
```

The first command builds Relay and Web, starts PostgreSQL, and creates the initial administrator. Open `http://127.0.0.1:17843` and sign in with the administrator credentials shown by `credentials`.

Repeat the deployment command to update the service. Use `make -C devmesh-relay status` to check the containers, `make -C devmesh-relay logs` to follow Relay logs, or `make -C devmesh-relay down` to stop the stack while retaining the database volume.

### Deploy to a server

The build machine needs Docker with Buildx, Python 3, Make, SSH, and SCP. The Linux server needs Docker with Compose v2, Python 3, and an SSH user who can run Docker and write to the deployment directory.

```sh
make deploy REMOTE_HOST=user@server
```

The command detects the server architecture, builds the matching Relay and Web image locally, transfers it over SSH, and starts it with PostgreSQL. The remote directory defaults to `/opt/devmesh`. Pass `REMOTE_DIR=/home/deploy/devmesh` to select a different writable absolute path.

Open `http://server:17843` using your server address and the administrator credentials printed during the first deployment. To retrieve the credentials again or inspect the service, run these commands in the remote deployment directory.

```sh
python3 manage.py credentials
python3 manage.py status
python3 manage.py logs
```

Deployments bind to `0.0.0.0:17843` by default. Set `RELAY_BIND_ADDRESS` or `RELAY_PORT` in the deployment `.env` and rerun the deployment to apply the change. The local `.env` lives in `devmesh-relay/deploy`, and the remote `.env` lives in `REMOTE_DIR`. Keep the `.env` and its `.local/bootstrap.json` record private. See the [Relay deployment reference](devmesh-relay/README.md) for image tags, platform overrides, and service management.

### Run the Relay binary directly

Build the workspace as described in [Develop locally](#develop-locally). Set `DATABASE_URL` to a PostgreSQL connection URL and put `psql` on `PATH`. Set `DEVMESH_INITIAL_PASSWORD` in the environment for the first bootstrap.

```sh
target/debug/devmesh-relay migrate
target/debug/devmesh-relay bootstrap --username admin
target/debug/devmesh-relay serve --web-dir devmesh-relay/web/dist
```

Run `bootstrap` once per database and store its device key securely. For an in-memory demo, run `target/debug/devmesh-relay serve --ephemeral --web-dir devmesh-relay/web/dist` with `DEVMESH_INITIAL_PASSWORD` set. The demo loses its data when the process exits.

## Install the Agent

On a macOS or Linux development machine, install Python 3, Make, Node.js 24+, npm, Git, and Go. Installing from source also requires Rust 1.89+. Use an interactive terminal with network access, a Gitea access token, and the Relay administrator credentials from deployment. On Linux, credential storage requires `secret-tool` and a running Secret Service.

Run the installer from the repository root or from an unpacked release bundle for your operating system and CPU architecture.

```sh
make install
```

Select Codex, Pi, Pi Durable, or a combination. The installer reuses selected runtimes it finds locally, downloads missing selections, and installs the Gitea MCP server. Selecting Codex registers the DevMesh skill and MCP integration. The Worker loads the Pi and Pi Durable adapters automatically.

Enter the Gitea URL and token, the Relay URL and administrator login, a workspace directory, and the roles this machine can handle. The wizard registers the device and stores credentials in macOS Keychain or Linux Secret Service. At the end, press Enter to start the Worker or enter `n` to start it later.

Sign in to the selected coding runtime using its native login flow. Open a new terminal so the installer’s `PATH` change takes effect. The default executable is `~/.local/share/devmesh/bin/devmesh`.

If you skipped setup, run `make configure` from the source checkout or `python3 setup_local.py` from the release bundle. Once configured, check connectivity.

```sh
devmesh doctor --verify
```

Use `make install INSTALL_ARGS='--prefix /absolute/path/to/devmesh'` for a custom prefix. The installer also accepts `--offline` inside `INSTALL_ARGS` for installation using locally available dependencies. See the [installation reference](docs/manual-install-test.md) and [example configuration](examples/config.json) for advanced settings and manual verification.

## Use DevMesh

List the repositories available to your Gitea token and check the Worker. If you skipped Worker startup during installation, run `devmesh worker start` first.

```sh
devmesh repos
devmesh worker status
```

Choose a `ready` issue whose required role matches the Agent. In a new Codex session, enter `$devmesh List my accessible Gitea repositories and their ready issues` to find one. To start with a new issue, use `$devmesh` to create a task with acceptance criteria and prepare it for execution in the target repository.

To start a specific ready issue from the terminal, substitute the numeric repository ID from `devmesh repos` and the Gitea issue number.

```sh
devmesh work --repository-id 42 --number 128
```

The Worker claims the issue, creates a Git worktree, and starts the selected runtime. Open the Relay web workspace to follow the session and its test results. For a code change, review the pull request and test evidence in Gitea, then merge the PR there. The Worker detects the merge and reconciles the issue.

Auto-claim and remote control are disabled by default. Use `devmesh configure` to edit Worker roles, then restart the Worker to apply changes. See [Worker automation](devmesh-agent/skills/devmesh/references/worker-automation.md) for configuring project-specific auto-claim.

## Develop locally

Install Rust 1.89+, Node.js 24+, npm, Git, Python 3, and Make. Populate the Cargo cache before running the workspace targets, which build and test Rust in offline mode.

```sh
cargo fetch --locked
npm ci --ignore-scripts
make build
make verify
python3 scripts/test_e2e.py
```

`make verify` checks formatting, Clippy, Rust and TypeScript tests, the web build, protocol and skill consistency, and isolated installation. It writes a report to `docs/verification/latest.json`. The end-to-end tests use local Git, SQLite, subprocesses, and test transports. Follow the [live validation guide](docs/live-validation.md) for a real deployment and task run.

| Directory | Contents |
| --- | --- |
| [`devmesh-agent`](devmesh-agent/) | CLI, Worker, installer, Gitea MCP, runtime adapters, and Codex skill |
| [`devmesh-protocol`](devmesh-protocol/) | Shared schemas, Rust and TypeScript types, REST client, and [OpenAPI contract](devmesh-protocol/openapi/devmesh.json) |
| [`devmesh-relay`](devmesh-relay/) | Relay service, PostgreSQL storage, deployment, and [web workspace](devmesh-relay/web/) |
| [`devmesh-v1.1`](devmesh-v1.1/) | [V1.1 design](devmesh-v1.1/DevMesh-V1.1.md) and protocol fixtures |

See the [Phase 1 functional scope](docs/phase1-functional.md), [implementation plan](docs/implementation-plan.md), and [execution map](docs/todos/DM-001/execution-map.json) for project details.
