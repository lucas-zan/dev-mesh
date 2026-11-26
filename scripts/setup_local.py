"""Interactive project setup using existing Gitea, Relay and credential contracts."""
import argparse
import getpass
import hashlib
import http.cookiejar
import json
import os
from pathlib import Path
import platform
import shlex
import subprocess
import urllib.error
import urllib.parse
import urllib.request
import uuid

ROLES = ('backend', 'frontend', 'product', 'qa')


class API:
    def __init__(self, base, token=None):
        self.base = base.rstrip('/')
        self.token = token
        self.opener = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def request(self, method, path, body=None):
        origin = urllib.parse.urlsplit(self.base)
        headers = {'Content-Type': 'application/json', 'Origin': f'{origin.scheme}://{origin.netloc}'}
        if self.token:
            headers['Authorization'] = 'token ' + self.token
        request = urllib.request.Request(self.base + path, method=method, headers=headers,
                                         data=json.dumps(body).encode() if body is not None else None)
        try:
            with self.opener.open(request, timeout=30) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            raise RuntimeError(f'{method} {path} failed: HTTP {error.code}') from error


def prompt(ask, label, default=''):
    value = ask(f'{label}' + (f' [{default}]' if default else '') + ': ').strip() or default
    if not value:
        raise ValueError(f'{label}不能为空')
    return value


def account(ask, secret, client):
    endpoint = prompt(ask, 'Gitea 地址', 'http://git.meditrust.xyz').rstrip('/')
    token = secret('Gitea Token（隐藏输入）: ').strip()
    if not token:
        raise ValueError('Gitea Token 不能为空')
    user = client(endpoint, token).request('GET', '/api/v1/user')
    return endpoint, token, user


def runtime_defaults(ask, profile, runtimes):
    workspace = Path(prompt(ask, '仓库统一工作目录', str(profile / 'devmesh-workspaces'))).expanduser().resolve()
    if not runtimes:
        raise ValueError('尚未选择 Runtime，请重新运行安装并选择 Agent')
    runtime = runtimes[0] if len(runtimes) == 1 else prompt(ask, '默认 Runtime：' + ' / '.join(runtimes), runtimes[0])
    if runtime not in runtimes:
        raise ValueError('Runtime 不在本次选择的 Agent 中')
    return {'workspace_root': str(workspace), 'runtime': runtime, 'auto_claim': False, 'test_commands': []}

def choose_roles(ask, current=()):
    print('可选角色：backend 后端、frontend 前端、product 产品、qa 测试。')
    answer = prompt(ask, '本机接单角色（逗号分隔，all 为全部）', ','.join(current)).lower()
    if answer == 'all':
        return list(ROLES)
    roles = list(dict.fromkeys(role.strip() for role in answer.split(',')))
    if any(role not in ROLES for role in roles):
        raise ValueError('请选择 backend、frontend、product、qa 中的一项或多项，或输入 all')
    return roles

def change_roles(path, ask=input):
    if not path.is_file():
        raise RuntimeError('缺少配置文件，请先运行 make install')
    config = json.loads(path.read_text())
    current = config.get('roles', [])
    roles = choose_roles(ask, current)
    if roles != current:
        config['roles'] = roles
        path.write_text(json.dumps(config, ensure_ascii=False, indent=2) + '\n')
        print(f'已保存接单角色：{", ".join(roles)}。若 Worker 正在运行，请在当前任务结束后重启。')
    else:
        print('接单角色保持不变。')

def store_credential(name, value, system=None, run=subprocess.run):
    system = system or platform.system()
    if system == 'Darwin':
        command = ['security', 'add-generic-password', '-U', '-a', 'devmesh', '-s', name, '-w', value]
        options, kind = {}, 'keychain:'
    elif system == 'Linux':
        command = ['secret-tool', 'store', '--label=DevMesh', 'service', name]
        options, kind = {'input': value}, 'secret-service:'
    else:
        raise RuntimeError('交互凭据保存目前支持 macOS Keychain 和 Linux Secret Service')
    try:
        run(command, check=True, capture_output=True, text=True, **options)
    except (subprocess.CalledProcessError, OSError):
        raise RuntimeError('无法写入系统凭据存储，请确认钥匙串/Secret Service 可用') from None
    return kind + name


def install_path(prefix, profile, shell=None):
    shell = shell or os.environ.get('SHELL', '/bin/zsh')
    if Path(shell).name == 'zsh':
        filename = '.zshrc'
    elif Path(shell).name == 'bash':
        filename = '.bash_profile' if platform.system() == 'Darwin' else '.bashrc'
    else:
        raise RuntimeError('自动 PATH 配置支持 zsh/bash，请用 --no-configure 单独安装')
    path = profile / filename
    content = path.read_text() if path.exists() else ''
    line = f'export PATH={shlex.quote(str(prefix / "bin"))}:"$PATH"'
    if line not in content.splitlines():
        profile.mkdir(parents=True, exist_ok=True)
        path.write_text(content + '\n# DevMesh\n' + line + '\n')
    os.environ['PATH'] = str(prefix / 'bin') + os.pathsep + os.environ.get('PATH', '')


def write_config(prefix, endpoint, token, relay_url, device, defaults, roles, save_secret):
    prefix.mkdir(parents=True, exist_ok=True)
    service = 'devmesh.' + hashlib.sha256(str(prefix).encode()).hexdigest()[:12]
    config = {'schema_version': 1, 'device_id': device['device_id'],
              'roles': roles,
              'gitea': {'endpoint': endpoint, 'credential_ref': save_secret(service + '.gitea', token),
                        'mcp_program': str(prefix / 'toolchains/gitea/bin/gitea-mcp')},
              'relay': {'endpoint': relay_url, 'credential_ref': save_secret(service + '.relay', device['key'])},
              'state_dir': str(prefix / 'state'), 'projects': [], 'repositories': defaults,
              'worker': {'enabled': True, 'max_concurrency': 2, 'poll_interval_seconds': 30,
                         'idle_grace_seconds': 120, 'approval_timeout_seconds': 900, 'input_timeout_seconds': 1800},
              'remote_control': {'enabled': False, 'allow_interrupt': False}}
    (prefix / 'config.json').write_text(json.dumps(config, ensure_ascii=False, indent=2) + '\n')
    return config


def preserve_config(prefix, profile):
    path = prefix / 'config.json'
    config = json.loads(path.read_text())
    if config.get('schema_version') == 1 and not config.get('repositories'):
        first = next(iter(config.get('projects', [])), {})
        endpoint = config['gitea']['endpoint'].rstrip('/')
        config['repositories'] = {
            'instance_id': first.get('repository', {}).get('instance_id') or str(uuid.uuid5(uuid.NAMESPACE_URL, endpoint)),
            'workspace_root': str(profile / 'devmesh-workspaces'), 'runtime': first.get('runtime', 'codex'),
            'auto_claim': False, 'test_commands': [],
        }
        path.write_text(json.dumps(config, ensure_ascii=False, indent=2) + '\n')
        print('已启用 Token 所授权的全部仓库；保留已有仓库路径、凭据和设备。')
    else:
        print(f'保留已有配置：{path}')
    install_path(prefix, profile)
    return config


def configure(prefix, profile, ask=input, secret=getpass.getpass, client=API,
              save_secret=store_credential, run=None):
    if (prefix / 'config.json').exists():
        return preserve_config(prefix, profile)
    print('DevMesh 配置引导：授权范围为 Gitea Token 可见的全部仓库，无需逐仓库登记。')
    endpoint, token, _user = account(ask, secret, client)
    relay_url = prompt(ask, 'Relay 地址', 'http://127.0.0.1:17843').rstrip('/')
    relay = client(relay_url)
    relay.request('POST', '/api/v1/auth/login', {
        'username': prompt(ask, 'Relay 管理员用户名', 'admin'),
        'password': secret('Relay 密码（隐藏输入）: '),
    })
    selection = prefix / 'agents.json'
    runtimes = json.loads(selection.read_text())['selected'] if selection.exists() else ['codex', 'pi', 'pi-durable']
    defaults = runtime_defaults(ask, profile, runtimes)
    roles = choose_roles(ask)
    existing = relay.request('GET', '/api/v1/admin/projects')['items']
    defaults['instance_id'] = next((item['instance_id'] for item in existing if item.get('gitea_endpoint') == endpoint),
                                   str(uuid.uuid5(uuid.NAMESPACE_URL, endpoint)))
    device = relay.request('POST', '/api/v1/admin/keys', {'name': platform.node() or 'DevMesh local'})
    config = write_config(prefix, endpoint, token, relay_url, device, defaults, roles, save_secret)
    install_path(prefix, profile)
    (run or (lambda args: subprocess.run(args, check=True)))([
        str(prefix / 'bin/devmesh'), 'doctor', '--prefix', str(prefix), '--verify'])
    print('配置已保存，Token 可见仓库将自动发现；PATH 在新终端生效。')
    print('新配置的自动认领默认关闭。')
    return config


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, default=Path(os.environ.get('DEVMESH_HOME', str(Path.home() / '.local/share/devmesh'))))
    parser.add_argument('--roles-config', type=Path, help='Edit the Worker roles in this config file')
    args = parser.parse_args()
    if args.roles_config is not None:
        change_roles(args.roles_config)
        return
    prefix = args.prefix.expanduser().resolve()
    path = prefix / 'config.json'
    if path.is_file():
        change_roles(path)
        return
    configure(prefix, Path.home())


if __name__ == '__main__':
    try:
        main()
    except (ValueError, RuntimeError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error))
