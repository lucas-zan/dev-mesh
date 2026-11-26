"""Build/install DevMesh and register its personal Codex Skill + MCP entry."""
import argparse
import datetime
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid
from setup_local import configure
from agent_selection import detect_agents, select_agents, download_names, reuse_agents

ROOT=Path(__file__).resolve().parents[1]

def execute(args,env=None):
    subprocess.run([str(arg) for arg in args],env=env,check=True)

def build_bundle(cache,offline,run=execute):
    version='0.1.0-local-'+datetime.datetime.now().strftime('%Y%m%d%H%M%S')+'-'+uuid.uuid4().hex[:6]
    bundle=cache/('devmesh-'+version)
    command=[sys.executable,str(ROOT/'scripts/package.py'),'--output',str(bundle),'--version',version]
    if not offline:command.append('--online')
    run(command)
    return bundle

def forward_credentials(settings,config):
    names=['GITEA_TOKEN','DEVMESH_RELAY_KEY']
    if config.is_file():
        values=json.loads(config.read_text())
        for key in ['gitea','relay']:
            reference=(values.get(key) or {}).get('credential_ref','')
            if reference.startswith('env:') and reference[4:] not in names:names.append(reference[4:])
    text=settings.read_text()
    match=re.search(r'^\[mcp_servers\.devmesh\]\s*$',text,re.M)
    if match is None:raise RuntimeError('Codex did not write the devmesh MCP section')
    end=re.search(r'^\[',text[match.end():],re.M)
    finish=match.end()+end.start() if end else len(text)
    block=text[match.start():finish]
    line='env_vars = '+json.dumps(names)+'\n'
    if re.search(r'^env_vars\s*=',block,re.M):block=re.sub(r'^env_vars\s*=.*$',line.rstrip(),block,flags=re.M)
    else:block=block.rstrip()+'\n'+line+'\n'
    settings.write_text(text[:match.start()]+block+text[finish:])

def register_codex(prefix,profile,program,run=execute,codex_home=None):
    source=prefix/'current/share/devmesh/skills/devmesh'
    if not (source/'SKILL.md').is_file():raise RuntimeError('Installed DevMesh Skill missing')
    target=profile/'.agents/skills/devmesh'
    target.parent.mkdir(parents=True,exist_ok=True)
    if target.is_symlink() and target.resolve()==source.resolve():pass
    elif target.exists() or target.is_symlink():raise RuntimeError(f'{target} already exists; move it before installing DevMesh')
    else:target.symlink_to(source,target_is_directory=True)
    config_dir=codex_home or profile/'.codex'
    config_dir.mkdir(parents=True,exist_ok=True)
    environment={**os.environ,'CODEX_HOME':str(config_dir)}
    run([program,'mcp','add','devmesh','--',str(prefix/'bin/devmesh'),'mcp',
         '--prefix',str(prefix),'--config',str(prefix/'config.json')],environment)
    forward_credentials(config_dir/'config.toml',prefix/'config.json')
    return {'skill':str(target),'mcp_profile':str(config_dir/'config.toml')}

def requirements(offline,needs_build,downloads=()):
    names=['node']
    if needs_build:names.append('cargo')
    if not offline:names.extend(['go','git'])
    if downloads:names.append('npm')
    missing=[name for name in names if not shutil.which(name)]
    if missing:raise RuntimeError('Install required tools first: '+', '.join(missing))

def arguments():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle','--source',dest='bundle',type=Path)
    parser.add_argument('--prefix',type=Path,default=Path(os.environ.get('DEVMESH_HOME',str(Path.home()/'.local/share/devmesh'))))
    parser.add_argument('--runtime',default='auto',help='auto, all, or comma-separated codex,pi,pi-durable')
    parser.add_argument('--host-home',type=Path)
    parser.add_argument('--codex-program')
    parser.add_argument('--skip-hosts',action='store_true')
    parser.add_argument('--offline',action='store_true')
    parser.add_argument('--configure',action=argparse.BooleanOptionalAction,default=None,
                        help='Guide Gitea Token/account setup (default: interactive online installation)')
    parser.add_argument('--start-worker', action=argparse.BooleanOptionalAction, default=None,
                        help='Start the configured Worker (default: ask in interactive setup)')
    return parser.parse_args()

def agent_plan(prefix,args):
    found=detect_agents(prefix)
    if args.offline and args.runtime=='auto' and not sys.stdin.isatty():selected=[]
    else:selected=select_agents(found,args.runtime,sys.stdin.isatty())
    downloads=download_names(selected,found,args.offline)
    for name in selected:
        if name in found:print(f'Reuse {name}: {found[name]["version"]} ({found[name]["packages"].get("pi-durable") or found[name]["program"]})')
        else:print(f'Install selected Agent: {name}')
    return found,selected,downloads

def register_selected_host(prefix,args,selected):
    if args.skip_hosts or 'codex' not in selected:return {}
    owned=prefix/'toolchains/codex/node_modules/.bin/codex'
    program=args.codex_program or shutil.which('codex') or (str(owned) if owned.is_file() else None)
    if program is None:raise RuntimeError('Codex CLI unavailable for MCP registration; use --skip-hosts for Worker-only installation')
    profile=args.host_home.expanduser().resolve() if args.host_home else Path.home()
    custom=Path(os.environ['CODEX_HOME']) if not args.host_home and os.environ.get('CODEX_HOME') else None
    return register_codex(prefix,profile,program,codex_home=custom)

def worker_running(status_command, run):
    try:
        status = run(status_command, capture_output=True, text=True, timeout=2)
        payload = json.loads(status.stdout if status.returncode == 0 else status.stderr)
    except (ValueError, subprocess.TimeoutExpired) as error:
        raise RuntimeError('无法读取 Worker 状态，请检查 daemon.log') from error
    if status.returncode == 0 and isinstance(payload.get('result', {}).get('works'), list):
        return True
    if payload.get('error', {}).get('code') == 'daemon_unavailable':
        return False
    message = payload.get('error', {}).get('message', '未知错误')
    raise RuntimeError(f'无法读取 Worker 状态：{message}')

def start_worker(prefix, config, run):
    command = [str(prefix / 'bin/devmesh'), 'worker']
    options = ['--prefix', str(prefix), '--config', str(prefix / 'config.json')]
    status_command = [*command, 'status', *options]
    if worker_running(status_command, run):
        return
    run([*command, 'start', *options], capture_output=True, text=True, check=True)
    for _ in range(20):
        if worker_running(status_command, run):
            return
        time.sleep(0.25)
    raise RuntimeError(f'Worker 启动后未就绪，请查看 {Path(config["state_dir"]) / "daemon.log"}')

def maybe_start_worker(prefix, choice, interactive, ask=input, run=subprocess.run):
    if choice is False or (choice is None and not interactive):
        return 'skipped'
    path = prefix / 'config.json'
    if not path.is_file():
        if choice:
            raise RuntimeError('无法启动 Worker：缺少 config.json，请先执行 make configure')
        return 'unconfigured'
    config = json.loads(path.read_text())
    if not config.get('worker', {}).get('enabled', False):
        if choice:
            raise RuntimeError('无法启动 Worker：配置中的 worker.enabled 未启用')
        return 'disabled'
    if choice is None:
        answer = ask('现在启动 DevMesh Worker？[Y/n] ').strip().lower()
        if answer in ('n', 'no'):
            return 'skipped'
        if answer not in ('', 'y', 'yes'):
            raise ValueError('请输入 y/yes 或 n/no')
    start_worker(prefix, config, run)
    return 'running'

def main():
    args=arguments()
    prefix=args.prefix.expanduser().resolve()
    found,selected,downloads=agent_plan(prefix,args)
    requirements(args.offline,args.bundle is None,downloads)
    bundle=args.bundle.resolve() if args.bundle else build_bundle(ROOT/'.cache/install-bundles',args.offline)
    command=[str(bundle/'bin/devmesh'),'install','--source',str(bundle),'--prefix',str(prefix),'--runtime',','.join(downloads) or 'none']
    if args.offline:command.append('--offline')
    execute(command)
    reuse_agents(prefix,selected,found)
    (prefix/'agents.json').write_text(json.dumps({'selected':selected,'detected':found},indent=2)+'\n')
    example=prefix/'current/share/devmesh/examples/config.json'
    if example.is_file() and not (prefix/'config.example.json').exists():shutil.copy2(example,prefix/'config.example.json')
    report={'prefix':str(prefix),'cli':str(prefix/'bin/devmesh'),'configuration':str(prefix/'config.example.json')}
    interactive=args.configure if args.configure is not None else sys.stdin.isatty() and not args.offline
    if interactive:
        profile=args.host_home.expanduser().resolve() if args.host_home else Path.home()
        configure(prefix,profile)
        report['configuration']=str(prefix/'config.json')
    report.update(register_selected_host(prefix,args,selected))
    execute([str(prefix/'bin/devmesh'),'doctor','--prefix',str(prefix)])
    report['worker'] = maybe_start_worker(prefix, args.start_worker, interactive and sys.stdin.isatty())
    print(json.dumps(report,indent=2))
    if report['worker'] == 'running':
        print('Worker 已就绪。选中 Codex 时，可在新会话中用 $devmesh 指定 Issue 开始工作。')
    elif report['worker'] == 'disabled':
        print('安装配置已完成。现有配置禁用了 Worker，可在 config.json 中启用 worker.enabled。')
    elif interactive:
        print('安装配置已完成。需要开发时，可运行 devmesh worker start。')
    else:print('Configuration not started. Run make configure from the source checkout, or python3 setup_local.py --prefix '+str(prefix)+' from the bundle.')
    print('Pi Extension and Pi Durable Host are installed and loaded by DevMesh Worker; no separate copy step.')

if __name__=='__main__':
    try:main()
    except (ValueError,RuntimeError,OSError,subprocess.CalledProcessError) as error:raise SystemExit(str(error))
