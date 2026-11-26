"""Detect and reuse the concrete programs/SDKs consumed by DevMesh adapters."""
import json
from pathlib import Path
import shutil
import subprocess

NAMES = ('codex', 'pi', 'pi-durable')
SDK_PACKAGES = ('pi-durable', 'pi-ai', 'chord')


def sdk_roots(prefix, which, run):
    roots = [prefix / 'current/share/devmesh/node_modules',
             Path.cwd() / 'devmesh-agent/packages/pi-durable-host/node_modules',
             Path.cwd() / 'node_modules']
    npm = which('npm')
    if npm:
        result = run([npm, 'root', '-g'], capture_output=True, text=True, check=False)
        if result.returncode == 0:
            roots.append(Path(result.stdout.strip()))
    return roots


def durable_packages(root):
    locations = [root, root / '@earendil-works/pi-durable/node_modules']
    locations.extend(parent / 'node_modules' for parent in root.parents)
    packages = {}
    for name in SDK_PACKAGES:
        match = next((directory / '@earendil-works' / name for directory in locations
                      if (directory / '@earendil-works' / name / 'package.json').is_file()), None)
        if match is None:
            return None
        packages[name] = match
    return packages


def detect_agents(prefix, which=shutil.which, run=subprocess.run, roots=None):
    found = {}
    for name in ('codex', 'pi'):
        owned = prefix / f'toolchains/{name}/node_modules/.bin/{name}'
        program = str(owned) if owned.is_file() else which(name)
        if not program:
            continue
        try:
            result = run([program, '--version'], capture_output=True, text=True, check=False)
        except OSError:
            continue
        if result.returncode == 0:
            found[name] = {'program': str(Path(program).resolve()),
                           'version': result.stdout.strip(), 'packages': {}}
    for root in roots if roots is not None else sdk_roots(prefix, which, run):
        packages = durable_packages(root)
        if packages:
            version = json.loads((packages['pi-durable'] / 'package.json').read_text())['version']
            found['pi-durable'] = {'program': which('node'), 'version': version,
                                   'packages': {name: str(path.resolve()) for name, path in packages.items()}}
            break
    return found


def agent_description(name, found):
    if name not in found:
        return '未安装，选中后下载'
    location = found[name]['packages'].get('pi-durable') or found[name]['program']
    return f'{found[name]["version"]} ({location})'


def choose_from_menu(found, ask):
    for index, name in enumerate(NAMES, 1):
        print(f'{index}. {name}: {agent_description(name, found)}')
    answer = ask('选择 Agent 序号，可用逗号多选: ').strip()
    try:
        indices = [int(value.strip()) for value in answer.split(',')]
        if any(index < 1 or index > len(NAMES) for index in indices):
            raise ValueError()
        return [NAMES[index - 1] for index in indices]
    except ValueError:
        raise ValueError('请选择有效的 Agent 序号') from None


def select_agents(found, requested='auto', interactive=False, ask=input):
    if requested != 'auto':
        selected = list(NAMES) if requested == 'all' else requested.split(',')
    elif not interactive:
        raise RuntimeError('Agent selection required; pass --runtime codex, pi, pi-durable or all')
    else:
        if len(found) == 1:
            name = next(iter(found))
            print(f'检测到唯一 Agent：{name}: {agent_description(name, found)}')
            answer = ask('是否复用此 Agent？[y/N]: ').strip().lower()
            if answer in ('y', 'yes'):
                return [name]
            if answer not in ('', 'n', 'no'):
                raise ValueError('请输入 y/yes 或 n/no')
        selected = choose_from_menu(found, ask)
    if any(name not in NAMES for name in selected):
        raise ValueError('Unsupported Agent selection')
    return list(dict.fromkeys(selected))


def download_names(selected, found, offline):
    missing = [name for name in selected if name not in found]
    if offline and missing:
        raise RuntimeError('offline: selected Agent is not installed: ' + ', '.join(missing))
    return missing


def link_owned(target, source):
    source = Path(source).resolve()
    if target.exists() and target.resolve() == source:
        return
    if target.is_symlink() or target.is_file():
        target.unlink()
    elif target.exists():
        raise RuntimeError(f'Existing owned package differs: {target}')
    target.parent.mkdir(parents=True, exist_ok=True)
    target.symlink_to(source, target_is_directory=source.is_dir())


def reuse_agents(prefix, selected, found):
    for name in selected:
        if name not in found:
            continue
        if name == 'pi-durable':
            for package, source in found[name]['packages'].items():
                link_owned(prefix / 'current/share/devmesh/node_modules/@earendil-works' / package, source)
        else:
            link_owned(prefix / f'toolchains/{name}/node_modules/.bin/{name}', found[name]['program'])
