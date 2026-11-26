import importlib.util
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('relay_deploy',ROOT/'devmesh-relay/deploy/manage.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class DeployTests(unittest.TestCase):
    def test_image_deploy_does_not_build_and_preserves_existing_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            env=Path(directory)/'.env'
            env.write_text('POSTGRES_PASSWORD=existing-db\nDEVMESH_INITIAL_PASSWORD=existing-admin\nRELAY_PORT=17844\n')
            calls=[]
            def execute(args,values,capture=False):
                calls.append((args,values))
                return subprocess.CompletedProcess(args,1 if capture else 0,'',json.dumps({'error':{'code':'already_initialized'}}))
            with contextlib.redirect_stdout(io.StringIO()):
                module.deploy(env,execute,lambda url:None,image='devmesh-relay:release')
            up=calls[0][0]
            self.assertIn('--no-build',up)
            self.assertNotIn('--build',up)
            self.assertEqual(calls[0][1]['RELAY_IMAGE'],'devmesh-relay:release')
            self.assertIn('POSTGRES_PASSWORD=existing-db',env.read_text())
            self.assertIn('RELAY_IMAGE=devmesh-relay:release',env.read_text())

    def test_deploy_builds_checks_health_and_bootstraps_only_once(self):
        with tempfile.TemporaryDirectory() as directory:
            env=Path(directory)/'.env'; calls=[]; bootstrap=[0]; health=[]
            def execute(args,values,capture=False):
                calls.append(args)
                if 'bootstrap' in args:
                    bootstrap[0]+=1
                    if bootstrap[0]>1:return subprocess.CompletedProcess(args,1,'',json.dumps({'error':{'code':'already_initialized'}}))
                    return subprocess.CompletedProcess(args,0,json.dumps({'user_id':'u','device_id':'d','token':'synthetic-key'}),'')
                return subprocess.CompletedProcess(args,0,'','')
            with contextlib.redirect_stdout(io.StringIO()):
                module.deploy(env,execute,lambda url:health.append(url))
            original=env.read_text()
            self.assertTrue(any('up' in args and '--build' in args and '--wait' in args for args in calls))
            self.assertEqual(health,['http://127.0.0.1:17843/healthz'])
            self.assertEqual(json.loads((env.parent/'.local/bootstrap.json').read_text())['device_id'],'d')
            with contextlib.redirect_stdout(io.StringIO()):
                module.deploy(env,execute,lambda url:None)
            self.assertEqual(env.read_text(),original)
            self.assertEqual(bootstrap[0],2)

    def test_health_failure_does_not_report_deployed_or_initialize_accounts(self):
        with tempfile.TemporaryDirectory() as directory:
            calls=[]
            def execute(args,values,capture=False):
                calls.append(args);return subprocess.CompletedProcess(args,0,'','')
            def fail(_):raise RuntimeError('health failed')
            with self.assertRaises(RuntimeError):module.deploy(Path(directory)/'.env',execute,fail)
            self.assertFalse(any('bootstrap' in args for args in calls))

    def test_compose_reuses_the_existing_postgres_17_tag_before_pulling(self):
        import os, shutil
        if not shutil.which('docker'):self.skipTest('Docker CLI not installed')
        result=subprocess.run(['docker','compose','-f',str(module.COMPOSE),'config','--format','json'],
                              env={**os.environ,'POSTGRES_PASSWORD':'synthetic-compose-check'},
                              capture_output=True,text=True,check=True)
        postgres=json.loads(result.stdout)['services']['postgres']
        self.assertEqual(postgres['image'],'postgres:17')
        self.assertEqual(postgres['pull_policy'],'missing')
        relay=json.loads(result.stdout)['services']['devmesh-relay']
        self.assertNotIn('build',relay)
        self.assertEqual(relay['image'],'devmesh-relay:local')
        with tempfile.TemporaryDirectory() as directory:
            env=Path(directory)/'.env'
            values=module.environment(env)
            result=module.execute(module.compose(env,build=True)+['config','--format','json'],values,True)
            self.assertEqual(result.returncode,0,result.stderr)
            local=json.loads(result.stdout)['services']['devmesh-relay']
            self.assertEqual(local['build']['dockerfile'],'devmesh-relay/deploy/Dockerfile')

if __name__=='__main__':unittest.main()
