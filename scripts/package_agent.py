"""Build the local release payload; no publication or credentials are required."""
from pathlib import Path
import hashlib,json,shutil,subprocess

root=Path(__file__).resolve().parents[1]
subprocess.run(['cargo','build','--release','-p','devmesh-agent','--offline'],cwd=root,check=True)
bundle=root/'target/bundle'
if bundle.exists(): shutil.rmtree(bundle)
(bundle/'bin').mkdir(parents=True)
shutil.copy2(root/'target/release/devmesh',bundle/'bin/devmesh')
assets=bundle/'share/devmesh';assets.mkdir(parents=True)
shutil.copytree(root/'devmesh-agent/skills',assets/'skills')
for name in ('codex-plugin','pi-extension','pi-durable-host'):
    shutil.copytree(root/'devmesh-agent/packages'/name,assets/'packages'/name,ignore=shutil.ignore_patterns('node_modules','test','dist'))
    if name!='pi-durable-host': shutil.copytree(root/'devmesh-agent/skills',assets/'packages'/name/'skills')
deps={name:'1.6.0' for name in ('@earendil-works/pi-durable','@earendil-works/pi-ai','@earendil-works/chord')}
(assets/'package.json').write_text(json.dumps({'name':'devmesh-runtime-dependencies','private':True,'type':'module','dependencies':deps},indent=2)+'\n')
files=[]
for path in sorted(bundle.rglob('*')):
    if path.is_file():files.append({'path':str(path.relative_to(bundle)),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'executable':path==bundle/'bin/devmesh'})
(bundle/'manifest.json').write_text(json.dumps({'version':'0.1.0','files':files},indent=2)+'\n')
print(bundle)
