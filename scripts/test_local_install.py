import json
from pathlib import Path
import tempfile
import unittest
from install_local import register_codex, build_bundle

class LocalInstallTests(unittest.TestCase):
    def test_skill_and_mcp_are_registered_in_the_selected_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);prefix=root/'prefix';profile=root/'profile'
            skill=prefix/'current/share/devmesh/skills/devmesh';skill.mkdir(parents=True)
            (skill/'SKILL.md').write_text('---\nname: devmesh\ndescription: test\n---\n')
            (profile/'notes').mkdir(parents=True);(profile/'notes/user.txt').write_text('keep')
            calls=[]
            def execute(args,env=None):
                calls.append((args,env))
                settings=Path(env['CODEX_HOME'])/'config.toml'
                settings.write_text('[mcp_servers.devmesh]\ncommand = "devmesh"\n')
            report=register_codex(prefix,profile,'codex-test',execute)
            link=profile/'.agents/skills/devmesh'
            self.assertTrue(link.is_symlink());self.assertEqual(link.resolve(),skill.resolve())
            args,env=calls[0]
            self.assertEqual(args[:4],['codex-test','mcp','add','devmesh'])
            self.assertIn(str(prefix/'config.json'),args)
            self.assertEqual(env['CODEX_HOME'],str(profile/'.codex'))
            register_codex(prefix,profile,'codex-test',execute)
            self.assertEqual((profile/'notes/user.txt').read_text(),'keep')
            self.assertEqual(report['skill'],str(link))

    def test_build_uses_a_distinct_installable_version_and_includes_online_fetch(self):
        with tempfile.TemporaryDirectory() as directory:
            commands=[]
            def execute(args,env=None):commands.append(args)
            first=build_bundle(Path(directory),False,execute)
            second=build_bundle(Path(directory),False,execute)
            self.assertNotEqual(first,second)
            self.assertIn('--online',commands[0])
            self.assertIn('--version',commands[0])

    def test_actual_codex_cli_preserves_other_mcp_configuration(self):
        import shutil, subprocess, tomllib
        program=shutil.which('codex')
        if not program:self.skipTest('Codex CLI not installed')
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);prefix=root/'prefix';profile=root/'profile'
            skill=prefix/'current/share/devmesh/skills/devmesh';skill.mkdir(parents=True)
            (skill/'SKILL.md').write_text('---\nname: devmesh\ndescription: test\n---\n')
            settings=profile/'.codex/config.toml';settings.parent.mkdir(parents=True)
            settings.write_text('model = "test-model"\n[mcp_servers.other]\ncommand = "other-server"\n')
            def execute(args,env=None):subprocess.run(args,env=env,check=True,capture_output=True,text=True)
            register_codex(prefix,profile,program,execute)
            values=tomllib.loads(settings.read_text())
            self.assertEqual(values['model'],'test-model')
            self.assertEqual(values['mcp_servers']['other']['command'],'other-server')
            self.assertEqual(values['mcp_servers']['devmesh']['command'],str(prefix/'bin/devmesh'))
            self.assertEqual(values['mcp_servers']['devmesh']['args'][0],'mcp')
            self.assertEqual(values['mcp_servers']['devmesh']['env_vars'],['GITEA_TOKEN','DEVMESH_RELAY_KEY'])

    def test_main_reuses_selected_agent_without_downloading_or_registering_other_hosts(self):
        from unittest.mock import patch
        import install_local
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);prefix=root/'prefix';prefix.mkdir()
            program=root/'pi';program.write_text('local-pi')
            found={'pi':{'program':str(program),'version':'local-pi','packages':{}}}
            calls=[]
            arguments=['install_local.py','--bundle',str(root/'bundle'),'--prefix',str(prefix),
                       '--runtime','pi','--no-configure']
            with patch('sys.argv',arguments),patch.object(install_local,'detect_agents',return_value=found),\
                 patch.object(install_local,'requirements'),patch.object(install_local,'execute',side_effect=lambda args:calls.append(args)),\
                 patch.object(install_local,'register_codex',side_effect=AssertionError('Codex not selected')):
                install_local.main()
            self.assertEqual(calls[0][calls[0].index('--runtime')+1],'none')
            self.assertEqual(json.loads((prefix/'agents.json').read_text())['selected'],['pi'])
            self.assertTrue((prefix/'toolchains/pi/node_modules/.bin/pi').is_symlink())

    def test_main_downloads_only_the_selected_missing_agent(self):
        from unittest.mock import patch
        import install_local
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);prefix=root/'prefix';prefix.mkdir();calls=[]
            arguments=['install_local.py','--bundle',str(root/'bundle'),'--prefix',str(prefix),
                       '--runtime','pi','--no-configure']
            with patch('sys.argv',arguments),patch.object(install_local,'detect_agents',return_value={}),\
                 patch.object(install_local,'requirements'),patch.object(install_local,'execute',side_effect=lambda args:calls.append(args)):
                install_local.main()
            self.assertEqual(calls[0][calls[0].index('--runtime')+1],'pi')

class InstallWorkerTests(unittest.TestCase):
    def setUp(self):
        from unittest.mock import Mock
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.prefix = Path(self.tmp.name)
        self.state = self.prefix / 'state'
        self.config = self.prefix / 'config.json'
        self.config.write_text(json.dumps({'worker': {'enabled': True},
                                           'state_dir': str(self.state),
                                           'repositories': {'auto_claim': False}}))
        self.run = Mock()
        self.ask = Mock(return_value='')

    def result(self, running):
        import subprocess
        if running:
            return subprocess.CompletedProcess([], 0, '{"result":{"works":[],"paused":false}}', '')
        return subprocess.CompletedProcess([], 1, '', '{"error":{"code":"daemon_unavailable"}}')

    def start(self, choice=None, interactive=True):
        from install_local import maybe_start_worker
        return maybe_start_worker(self.prefix, choice, interactive, ask=self.ask, run=self.run)

    def test_enter_starts_worker_and_waits_for_status_without_changing_auto_claim(self):
        from unittest.mock import patch
        self.run.side_effect = [self.result(False), self.result(True), self.result(False), self.result(True)]
        initial = self.config.read_text()
        with patch('install_local.time.sleep'):
            self.assertEqual(self.start(), 'running')
        self.assertIn('[Y/n]', self.ask.call_args.args[0])
        self.assertEqual([call.args[0][2] for call in self.run.call_args_list],
                         ['status', 'start', 'status', 'status'])
        self.assertEqual(self.config.read_text(), initial)

    def test_decline_and_noninteractive_defaults_skip(self):
        self.ask.return_value = 'n'
        self.assertEqual(self.start(), 'skipped')
        self.ask.reset_mock()
        self.assertEqual(self.start(interactive=False), 'skipped')
        self.assertEqual(self.start(choice=False), 'skipped')
        self.ask.assert_not_called()
        self.run.assert_not_called()

    def test_already_running_is_reused_without_launching_again(self):
        self.run.return_value = self.result(True)
        self.assertEqual(self.start(choice=True, interactive=False), 'running')
        self.ask.assert_not_called()
        self.run.assert_called_once()
        self.assertEqual(self.run.call_args.args[0][2], 'status')

    def test_missing_or_disabled_config_explicit_start_fails(self):
        self.config.unlink()
        self.assertEqual(self.start(), 'unconfigured')
        with self.assertRaisesRegex(RuntimeError, 'config'):
            self.start(choice=True, interactive=False)
        self.config.write_text(json.dumps({'worker': {'enabled': False}}))
        self.assertEqual(self.start(), 'disabled')
        with self.assertRaisesRegex(RuntimeError, 'worker.enabled'):
            self.start(choice=True, interactive=False)
        self.run.assert_not_called()

    def test_timeout_points_to_worker_log(self):
        from unittest.mock import patch
        self.run.return_value = self.result(False)
        with patch('install_local.time.sleep'):
            with self.assertRaisesRegex(RuntimeError, 'daemon.log'):
                self.start()

    def test_invalid_confirmation_is_not_treated_as_consent(self):
        self.ask.return_value = 'maybe'
        with self.assertRaises(ValueError):
            self.start()
        self.run.assert_not_called()


if __name__=='__main__':unittest.main()
