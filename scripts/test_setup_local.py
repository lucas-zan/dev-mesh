"""Token-scoped setup without per-repository enrollment or real services."""
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from setup_local import API, account, configure, install_path, runtime_defaults, store_credential


class SetupTests(unittest.TestCase):
    def scenario(self, prefix, profile, fail=False):
        prefix.mkdir(parents=True, exist_ok=True)
        (prefix/'agents.json').write_text('{"selected":["codex"]}')
        answers=iter(['','http://relay.test:17843','','','backend,qa'])
        passwords=iter(['gitea-synthetic','relay-password'])
        calls,secrets,commands,prompts=[],{},[],[]
        class Client:
            def __init__(self, base, token=None):self.base=base
            def request(self, method, path, body=None):
                calls.append((self.base,method,path,body))
                if fail:raise RuntimeError('Gitea rejected token')
                if path=='/api/v1/user':return {'id':1}
                if path=='/api/v1/auth/login':return {'authenticated':True}
                if path=='/api/v1/admin/projects':return {'items':[]}
                if path=='/api/v1/admin/keys':return {'device_id':'11111111-1111-4111-8111-111111111111','key':'device-synthetic'}
                raise AssertionError(path)
        def ask(label):prompts.append(label);return next(answers)
        def save(name,value):secrets[name]=value;return 'keychain:'+name
        config=configure(prefix,profile,ask=ask,secret=lambda _:next(passwords),client=Client,
                         save_secret=save,run=lambda args:commands.append(args))
        return config,calls,secrets,commands,prompts

    def test_install_configures_the_token_scope_without_any_repository_prompt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);prefix=root/'prefix'
            config,calls,secrets,commands,prompts=self.scenario(prefix,root/'profile')
            self.assertEqual(config['roles'],['backend','qa'])
            self.assertEqual(config['projects'],[])
            self.assertEqual(config['repositories']['runtime'],'codex')
            self.assertEqual(config['repositories']['workspace_root'],str((root/'profile/devmesh-workspaces').resolve()))
            self.assertFalse(config['repositories']['auto_claim'])
            self.assertEqual(config['device_id'],'11111111-1111-4111-8111-111111111111')
            self.assertNotIn('synthetic',(prefix/'config.json').read_text())
            self.assertEqual(set(secrets.values()),{'gitea-synthetic','device-synthetic'})
            self.assertIn('--verify',commands[-1])
            self.assertFalse(any('owner' in label or '测试命令' in label for label in prompts))
            self.assertFalse(any(method=='POST' and path.endswith('/projects') for _,method,path,_ in calls))

    def test_gitea_default_can_be_overridden_and_token_uses_secret_input(self):
        for address,expected in [('', 'http://git.meditrust.xyz'),('https://other.test/','https://other.test')]:
            prompts=[];tokens=[]
            class Client:
                def __init__(self, endpoint, token):tokens.append(token)
                def request(self, method, path):
                    self.assert_path=path
                    return {'id':1}
            def ask(label):prompts.append(label);return address
            result=account(ask,lambda _:'synthetic-token',Client)
            self.assertEqual(result[0],expected)
            self.assertIn('http://git.meditrust.xyz',prompts[0])
            self.assertEqual(tokens,['synthetic-token'])
            self.assertEqual(len(prompts),1)

    def test_role_change_preserves_other_config_and_validates_before_writing(self):
        from setup_local import change_roles
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'config.json'
            old = {'roles':['backend'], 'worker':{'enabled':True}, 'device_id':'existing'}
            path.write_text(json.dumps(old))
            change_roles(path, ask=lambda _: '')
            self.assertEqual(path.read_text(), json.dumps(old))
            with self.assertRaises(ValueError):
                change_roles(path, ask=lambda _: 'frontend,invalid')
            self.assertEqual(json.loads(path.read_text()), old)
            change_roles(path, ask=lambda _: 'frontend, qa, frontend')
            self.assertEqual(json.loads(path.read_text()), {**old, 'roles':['frontend','qa']})
            change_roles(path, ask=lambda _: 'all')
            self.assertEqual(json.loads(path.read_text())['roles'], ['backend','frontend','product','qa'])

    def test_existing_discovery_configuration_is_preserved_without_prompts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);path=root/'config.json'
            text='{"repositories":{"runtime":"pi"},"projects":[]}\n';path.write_text(text)
            configure(root,root,ask=lambda _:self.fail('must not prompt'))
            self.assertEqual(path.read_text(),text)

    def test_existing_bound_configuration_is_upgraded_to_all_token_repositories(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);path=root/'config.json'
            old={'schema_version':1,'gitea':{'endpoint':'http://git.test','credential_ref':'keychain:keep'},
                 'device_id':'keep-device','projects':[{'runtime':'pi','repository':{'instance_id':'22222222-2222-4222-8222-222222222222'}}]}
            path.write_text(json.dumps(old))
            config=configure(root,root,ask=lambda _:self.fail('must not prompt'))
            self.assertIn('repositories',config)
            self.assertEqual(config['repositories']['runtime'],'pi')
            self.assertEqual(config['gitea'],old['gitea'])
            self.assertEqual(config['device_id'],'keep-device')
            self.assertEqual(config['projects'],old['projects'])

    def test_failed_token_does_not_write_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            with self.assertRaisesRegex(RuntimeError,'rejected'):self.scenario(root/'prefix',root/'profile',True)
            self.assertFalse((root/'prefix/config.json').exists())

    def test_runtime_defaults_reuse_single_selection_and_reject_unselected_agent(self):
        defaults=runtime_defaults(lambda _:'',Path('/tmp/profile'),['pi'])
        self.assertEqual(defaults['runtime'],'pi')
        answers=iter(['','pi-durable'])
        with self.assertRaisesRegex(ValueError,'Runtime'):
            runtime_defaults(lambda _:next(answers),Path('/tmp/profile'),['codex','pi'])

    def test_path_is_idempotent_and_quotes_prefix(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            install_path(root/'space dir',root,shell='/bin/zsh');install_path(root/'space dir',root,shell='/bin/zsh')
            text=(root/'.zshrc').read_text()
            self.assertEqual(text.count('export PATH='),1)
            self.assertIn('"$PATH"',text)

    def test_credential_provider_matches_reader_and_does_not_leak_failed_token(self):
        calls=[]
        self.assertEqual(store_credential('devmesh.test','synthetic',system='Darwin',
                         run=lambda args,**kwargs:calls.append(args)),'keychain:devmesh.test')
        self.assertIn('add-generic-password',calls[0])
        def fail(args,**kwargs):raise subprocess.CalledProcessError(1,args)
        with self.assertRaises(RuntimeError) as caught:
            store_credential('devmesh.test','private-synthetic',system='Darwin',run=fail)
        self.assertNotIn('private-synthetic',str(caught.exception))

    def test_relay_cookie_writes_supply_the_required_origin(self):
        client=API('http://relay.test:17843');calls=[]
        class Opener:
            def open(self, request, timeout):calls.append(request);return io.BytesIO(b'{}')
        client.opener=Opener();client.request('POST','/api/v1/auth/login',{'username':'admin'})
        self.assertEqual(calls[0].get_header('Origin'),'http://relay.test:17843')

    def test_config_is_accepted_by_actual_cli_without_any_bound_repository(self):
        cli=Path(__file__).resolve().parents[1]/'target/debug/devmesh'
        if not cli.exists():self.skipTest('Build CLI first')
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);prefix=root/'prefix'
            self.scenario(prefix,root/'profile')
            subprocess.run([str(cli),'init','--from',str(prefix/'config.json'),'--config',str(root/'checked.json'),
                            '--prefix',str(prefix)],capture_output=True,text=True,check=True)
            checked=json.loads((root/'checked.json').read_text())
            self.assertEqual(checked['projects'],[])
            self.assertEqual(checked['repositories']['runtime'],'codex')


if __name__=='__main__':unittest.main()
