"""Install the actual CLI bundle into an isolated prefix; never touch the user's HOME."""
import json
import subprocess
import tempfile
import unittest
import os
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
class ReleaseTests(unittest.TestCase):
 def test_install_doctor_update_rollback_uninstall(self):
  with tempfile.TemporaryDirectory(prefix='devmesh-install-') as temporary:
   work=Path(temporary);staged=work/'devmesh-0.1.0';prefix=work/'prefix'
   subprocess.run(['python3','scripts/package.py','--debug','--output',str(staged)],cwd=ROOT,check=True)
   archive = work/'devmesh-0.1.0.tar.gz'
   self.assertTrue(archive.exists())
   unpacked = work/'unpacked release'
   unpacked.mkdir()
   subprocess.run(['tar', '-xzf', str(archive), '-C', str(unpacked)], check=True)
   bundle = unpacked/staged.name
   self.assertTrue((bundle/'setup_local.py').exists())
   self.assertTrue((bundle/'agent_selection.py').exists())
   cli=bundle/'bin/devmesh'
   def run(*args):
    return subprocess.run([str(cli),*args,'--prefix',str(prefix)],check=True,capture_output=True,text=True)
   install_args = f'INSTALL_ARGS=--prefix "{prefix}" --offline --skip-hosts'
   subprocess.run(['make', 'install', install_args], cwd=bundle, check=True, text=True)
   self.assertTrue((prefix/'current/bin/devmesh').exists())
   self.assertTrue((prefix/'config.example.json').exists())
   self.assertFalse((prefix/'config.json').exists())
   # Runtime reuse must still install Gitea MCP without invoking npm.
   tools=work/'tools';tools.mkdir()
   probe=work/'go-called'
   (tools/'go').write_text('#!/bin/sh\nprintf invoked > "$DEVMESH_INSTALL_PROBE"\n')
   (tools/'npm').write_text('#!/bin/sh\nexit 88\n')
   (tools/'go').chmod(0o755);(tools/'npm').chmod(0o755)
   subprocess.run([str(cli),'install','--source',str(bundle),'--prefix',str(prefix),'--runtime','none'],
                  env={**os.environ,'PATH':str(tools)+os.pathsep+os.environ.get('PATH',''),
                       'DEVMESH_INSTALL_PROBE':str(probe)},check=True,capture_output=True,text=True)
   self.assertEqual(probe.read_text(),'invoked')
   (prefix/'config.json').write_text('{"user":"preserved"}')
   report=json.loads(run('doctor').stdout);self.assertFalse(report['runtimes'][0].get('native_e2e') is True)
   self.assertFalse(next(runtime for runtime in report['runtimes'] if runtime['runtime']=='pi-durable')['installed'])
   self.verify_worker_start(bundle, prefix, run)
   run('update','--source',str(bundle),'--offline');run('rollback','--version','0.1.0');run('uninstall')
   self.assertTrue((prefix/'config.json').exists());self.assertFalse((prefix/'bin/devmesh').exists())

 def verify_worker_start(self, bundle, prefix, run):
  with tempfile.TemporaryDirectory(prefix='dm-worker-', dir='/tmp') as state:
   path = prefix/'config.json'
   config = json.loads((prefix/'config.example.json').read_text())
   config.update(state_dir=state, relay=None, repositories=None, projects=[])
   config['worker']['enabled'] = True
   path.write_text(json.dumps(config))
   subprocess.run([str(prefix/'bin/devmesh'), 'configure', '--prefix', str(prefix)],
                  input='frontend,qa\n', check=True, capture_output=True, text=True, timeout=5)
   self.assertEqual(json.loads(path.read_text()), {**config, 'roles':['frontend','qa']})
   command = ['make', 'install', f'INSTALL_ARGS=--prefix "{prefix}" --offline --skip-hosts --start-worker']
   try:
    result = subprocess.run(command, cwd=bundle, check=True, capture_output=True, text=True, timeout=20)
    self.assertIn('"worker": "running"', result.stdout)
    self.assertEqual(json.loads(run('worker', 'status').stdout)['result']['works'], [])
   finally:
    if (Path(state)/'daemon.sock').exists():
     run('worker', 'stop')

if __name__=='__main__':unittest.main()
