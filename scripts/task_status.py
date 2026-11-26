"""Update one execution-map node after recording its actual verification."""
import argparse
import datetime
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('node')
parser.add_argument('state', choices=['start','pass','fail'])
parser.add_argument('--command', action='append', default=[])
parser.add_argument('--note', default='')
args=parser.parse_args()
base=Path(__file__).resolve().parents[1]/'docs/todos/DM-001'
path=base/'execution-map.json'
graph=json.loads(path.read_text())
node=next(n for n in graph['nodes'] if n['id']==args.node)
if args.state=='start':
    assert node['readiness']=='READY' and not node['blockers']
node['status']={'start':'待测试验证','pass':'验证成功','fail':'验证失败'}[args.state]
now=datetime.datetime.now(datetime.timezone.utc).isoformat()
for command in args.command:
    node['evidence'].append(dict(command=command,result='PASS' if args.state=='pass' else 'FAIL',scope='local',verified_at=now))
for item in graph['nodes']:
    ready=all(next(n for n in graph['nodes'] if n['id']==d)['status']=='验证成功' for d in item['depends_on'])
    item['readiness']='READY' if ready and not item['blockers'] and item['maturity']=='DECOMPOSED' else 'BLOCKED'
path.write_text(json.dumps(graph,ensure_ascii=False,indent=2)+'\n')
if args.note:
    todo=base/node['todo']
    text=todo.read_text()
    if args.state=='pass': text=text.replace('- [ ]','- [x]')
    todo.write_text(text+f'\n## Evidence {now}\n\n{args.note}\n')
