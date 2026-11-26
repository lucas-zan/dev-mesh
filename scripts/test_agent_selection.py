import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from agent_selection import detect_agents, select_agents, reuse_agents, download_names


class AgentSelectionTests(unittest.TestCase):
    def detected(self, **programs):
        return {name: {'program': program, 'version': '1.0', 'packages': {}}
                for name, program in programs.items()}

    def test_detects_executables_with_versions_and_rejects_failed_probe(self):
        calls=[]
        def run(args,**kwargs):
            calls.append(args)
            return subprocess.CompletedProcess(args,1 if args[0]=='/bin/pi' else 0,'codex 0.172.0\n','')
        with tempfile.TemporaryDirectory() as temporary:
            result=detect_agents(Path(temporary),which=lambda name:{'codex':'/bin/codex','pi':'/bin/pi'}.get(name),run=run,roots=[])
        self.assertEqual(result['codex']['program'],'/bin/codex')
        self.assertNotIn('pi',result)
        self.assertNotIn('pi-durable',result)
        self.assertEqual(result['codex']['version'],'codex 0.172.0')

    def test_detects_durable_sdk_and_reuses_all_host_dependencies(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); modules=root/'modules'
            for name in ('pi-durable','pi-ai','chord'):
                package=modules/'@earendil-works'/name;package.mkdir(parents=True)
                (package/'package.json').write_text(json.dumps({'version':'1.6.0'}))
            found=detect_agents(root/'prefix',which=lambda _:None,roots=[modules])
            self.assertIn('pi-durable',found)
            (root/'prefix/current/share/devmesh').mkdir(parents=True)
            reuse_agents(root/'prefix',['pi-durable'],found)
            self.assertEqual((root/'prefix/current/share/devmesh/node_modules/@earendil-works/pi-durable').resolve(),
                             (modules/'@earendil-works/pi-durable').resolve())

    def test_one_agent_is_reused_only_after_confirmation_and_shows_details(self):
        import contextlib, io
        only=self.detected(codex='/bin/codex')
        output=io.StringIO(); prompts=[]
        def confirm(prompt):
            prompts.append(prompt)
            return 'yes'
        with contextlib.redirect_stdout(output):
            selected=select_agents(only,interactive=True,ask=confirm)
        self.assertEqual(selected,['codex'])
        self.assertEqual(len(prompts),1)
        for detail in ('codex','1.0','/bin/codex'):
            self.assertIn(detail,output.getvalue())

    def test_declining_or_entering_blank_allows_selecting_another_agent(self):
        for decision in ('n',''):
            answers=iter([decision,'2'])
            result=select_agents(self.detected(codex='/bin/codex'),interactive=True,
                                 ask=lambda _:next(answers))
            self.assertEqual(result,['pi'])

    def test_noninteractive_single_agent_requires_explicit_selection(self):
        only=self.detected(codex='/bin/codex')
        with self.assertRaisesRegex(RuntimeError,'--runtime'):
            select_agents(only,interactive=False)
        self.assertEqual(select_agents(only,requested='codex',interactive=False),['codex'])

    def test_invalid_confirmation_is_an_explicit_error(self):
        with self.assertRaises(ValueError):
            select_agents(self.detected(codex='/bin/codex'),interactive=True,ask=lambda _:'maybe')

    def test_multiple_agents_require_selection(self):
        multiple=self.detected(codex='/bin/codex',pi='/bin/pi')
        self.assertEqual(select_agents(multiple,interactive=True,ask=lambda _:'1,2'),['codex','pi'])
        with self.assertRaisesRegex(RuntimeError,'--runtime'):
            select_agents(multiple,interactive=False)

    def test_missing_agents_are_downloaded_only_when_selected(self):
        found=self.detected(codex='/bin/codex')
        selected=select_agents(found,requested='pi',interactive=False)
        self.assertEqual(download_names(selected,found,offline=False),['pi'])
        self.assertEqual(download_names(['codex'],found,offline=False),[])
        with self.assertRaisesRegex(RuntimeError,'offline'):
            download_names(selected,found,offline=True)
        self.assertEqual(select_agents({},interactive=True,ask=lambda _:'2'),['pi'])
        with self.assertRaisesRegex(RuntimeError,'--runtime'):
            select_agents({},interactive=False)

    def test_reused_program_runs_from_the_existing_worker_path_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);program=root/'my codex'
            program.write_text('#!/bin/sh\nprintf "local codex\\n"\n');program.chmod(0o755)
            found=self.detected(codex=str(program))
            reuse_agents(root/'prefix',['codex'],found)
            reuse_agents(root/'prefix',['codex'],found)
            target=root/'prefix/toolchains/codex/node_modules/.bin/codex'
            result=subprocess.run([str(target),'--version'],capture_output=True,text=True,check=True)
            self.assertEqual(result.stdout,'local codex\n')
            self.assertTrue(program.exists())

    def test_reused_real_durable_sdk_starts_the_packaged_host(self):
        import shutil
        root=Path(__file__).resolve().parents[1]
        found=detect_agents(root/'.cache/unused-detection-prefix',roots=[
            root/'devmesh-agent/packages/pi-durable-host/node_modules'])
        if 'pi-durable' not in found:self.skipTest('Pi Durable SDK not cached')
        with tempfile.TemporaryDirectory() as temporary:
            prefix=Path(temporary);share=prefix/'current/share/devmesh'
            destination=share/'packages/pi-durable-host/src'
            shutil.copytree(root/'devmesh-agent/packages/pi-durable-host/src',destination)
            (share/'package.json').write_text('{"type":"module"}')
            reuse_agents(prefix,['pi-durable'],found)
            result=subprocess.run(['node',str(destination/'cli.ts')],input='',text=True,
                                  capture_output=True,timeout=30,check=True)
            hello=json.loads(result.stdout.splitlines()[0])
            self.assertEqual(hello['type'],'hello')
            self.assertEqual(hello['runtime'],'pi-durable')

    def test_sdk_detection_respects_workspace_parent_module_resolution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);local=root/'packages/host/node_modules'
            for name, modules in [('pi-durable',root/'node_modules'),
                                  ('pi-ai',root/'node_modules'),('chord',local)]:
                package=modules/'@earendil-works'/name;package.mkdir(parents=True)
                (package/'package.json').write_text('{"version":"1.6.0"}')
            result=detect_agents(root/'prefix',which=lambda _:None,roots=[local])
            self.assertIn('pi-durable',result)


if __name__=='__main__':unittest.main()
