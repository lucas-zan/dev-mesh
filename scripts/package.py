"""Build a platform-specific, checksummed DevMesh bundle from repository sources."""
import argparse
import hashlib
import json
import platform
import shutil
import subprocess
import tarfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
BUNDLE_MAKEFILE = """\
.PHONY: install
install:
\tpython3 ./install-local.py --bundle "$(CURDIR)" $(INSTALL_ARGS)
"""

def copy_tree(source,destination):
 for path in source.rglob('*'):
  relative=path.relative_to(source)
  if any(part in ('node_modules','dist','.test-build','__pycache__') for part in relative.parts):continue
  if path.is_file():
   target=destination/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(path,target)
def package(output,debug,version='0.1.0',offline=True):
 args=['cargo','build','--workspace','--locked']
 if offline:args.append('--offline')
 if not debug:args.append('--release')
 subprocess.run(args,cwd=ROOT,check=True)
 output.mkdir(parents=True,exist_ok=True)
 if (output/'manifest.json').exists():raise RuntimeError('Choose a fresh output directory')
 binary=ROOT/'target'/('debug' if debug else 'release')
 for name in ['devmesh','devmesh-relay']:
  target=output/'bin'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(binary/name,target)
 shutil.copy2(ROOT/'scripts/install.sh',output/'install.sh')
 shutil.copy2(ROOT/'scripts/install_local.py',output/'install-local.py')
 shutil.copy2(ROOT/'scripts/setup_local.py',output/'setup_local.py')
 shutil.copy2(ROOT/'scripts/agent_selection.py',output/'agent_selection.py')
 (output/'Makefile').write_text(BUNDLE_MAKEFILE)
 (output/'README.md').write_text('# DevMesh 本机安装包\n\n在解压后的包目录运行 `make install`。\n\n安装器使用包里的 DevMesh 二进制，引导选择 Agent、配置 Gitea 和 Relay，并询问是否启动 Worker。\n配置与人工验证步骤见 [安装测试](docs/manual-install-test.md)。\n')
 for name in ['manual-install-test.md','live-validation.md','phase1-functional.md','runtime-compatibility.md']:
  target=output/'docs'/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(ROOT/'docs'/name,target)
 share=output/'share/devmesh'
 for path in ['skills','resources','packages/pi-extension','packages/pi-durable-host','.codex-plugin']:
  source=ROOT/'devmesh-agent'/path
  if source.exists():copy_tree(source,share/path)
 copy_tree(ROOT/'devmesh-agent/skills',share/'packages/pi-extension/skills')
 copy_tree(ROOT/'examples',share/'examples')
 dependencies=json.load(open(ROOT/'devmesh-agent/packages/pi-durable-host/package.json'))['dependencies']
 (share/'package.json').write_text(json.dumps({'name':'devmesh-runtime-bundle','version':'0.1.0','private':True,'type':'module','dependencies':dependencies},indent=2)+'\n')
 if (ROOT/'devmesh-relay/web/dist').exists():copy_tree(ROOT/'devmesh-relay/web/dist',output/'share/devmesh/web')
 files=[]
 for path in sorted(output.rglob('*')):
  if not path.is_file():continue
  files.append({'path':path.relative_to(output).as_posix(),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'executable':bool(path.stat().st_mode&0o111)})
 manifest={'version':version,'platform':platform.system().lower(),'architecture':platform.machine(),'build_profile':'debug' if debug else 'release','files':files}
 (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
 archive=output.parent/(output.name+'.tar.gz')
 with tarfile.open(archive,'w:gz') as target:target.add(output,arcname=output.name)
 archive.with_suffix(archive.suffix+'.sha256').write_text(f'{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n')
 print(json.dumps({'bundle':str(output),'archive':str(archive),'files':len(files)}))
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--debug',action='store_true');parser.add_argument('--online',action='store_true');parser.add_argument('--version',default='0.1.0');parser.add_argument('--output',type=Path,required=True);args=parser.parse_args();package(args.output.resolve(),args.debug,args.version,not args.online)
