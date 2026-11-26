"""Run local gates and retain their actual exit codes as reviewable evidence."""
import datetime
import json
import subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
commands=[['cargo','fmt','--all','--','--check'],['cargo','clippy','--workspace','--all-targets','--offline','--','-D','warnings'],['cargo','test','--workspace','--offline'],['npm','test','--workspaces','--if-present'],['npm','run','build','--workspaces','--if-present'],['python3','scripts/test_deploy.py'],['python3','scripts/test_remote_deploy.py'],['python3','scripts/test_local_install.py'],['python3','scripts/test_setup_local.py'],['python3','scripts/test_agent_selection.py'],['python3','scripts/test_live_validation.py'],['python3','scripts/test_api_contract.py'],['python3','scripts/test_skill.py'],['python3','scripts/test_release.py']]
results=[]
for command in commands:
 print('+ '+' '.join(command),flush=True)
 result=subprocess.run(command,cwd=ROOT)
 results.append(dict(command=command,exit_code=result.returncode))
 if result.returncode:break
report=dict(verified_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),scope='Local code gates; excludes live Gitea/model/TCP/PostgreSQL/browser acceptance',results=results,passed=len(results)==len(commands) and all(r['exit_code']==0 for r in results))
output=ROOT/'docs/verification/latest.json';output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n')
raise SystemExit(0 if report['passed'] else 1)
