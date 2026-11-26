"""Publishing behavior at the local command / SSH boundary."""
import importlib.util
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('remote_deploy', ROOT / 'devmesh-relay/deploy/remote.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class RemoteDeployTests(unittest.TestCase):
    def runner(self, architecture='x86_64', fail_build=False):
        calls = []
        def run(args, **kwargs):
            calls.append(args)
            if fail_build and 'buildx' in args:
                raise subprocess.CalledProcessError(1, args)
            return subprocess.CompletedProcess(args, 0, architecture + '\n', '')
        return calls, run

    def test_builds_locally_for_remote_arch_and_transfers_only_runtime_files(self):
        calls, run = self.runner()
        module.deploy('test-host', '/srv/dev mesh', 'devmesh-relay:test', run=run)
        build = next(args for args in calls if 'buildx' in args)
        self.assertEqual(build[build.index('--platform') + 1], 'linux/amd64')
        self.assertIn('--load', build)
        self.assertTrue(any(args[:3] == ['docker', 'image', 'save'] for args in calls))
        copy = next(args for args in calls if args[0] == 'scp')
        self.assertEqual(copy[-1], 'test-host:/srv/dev mesh/')
        self.assertTrue(any(arg.endswith('/compose.yaml') for arg in copy))
        self.assertTrue(any(arg.endswith('/manage.py') for arg in copy))
        self.assertFalse(any('Dockerfile' in arg or 'compose.build' in arg for arg in copy))
        command = calls[-1][-1]
        self.assertIn('docker load', command)
        self.assertIn('deploy-image', command)
        self.assertIn("'/srv/dev mesh'", command)
        self.assertNotIn('--build', command)

    def test_arm_and_explicit_platform(self):
        for architecture, override, expected in [('aarch64', None, 'linux/arm64'),
                                                   ('unused', 'linux/amd64', 'linux/amd64')]:
            calls, run = self.runner(architecture)
            module.deploy('host', '/opt/devmesh', 'devmesh:test', platform=override, run=run)
            build = next(args for args in calls if 'buildx' in args)
            self.assertEqual(build[build.index('--platform') + 1], expected)

    def test_missing_host_and_unknown_arch_fail_before_build(self):
        calls, run = self.runner('mips')
        with self.assertRaisesRegex(ValueError, 'REMOTE_HOST'):
            module.deploy('', '/opt/devmesh', 'devmesh:test', run=run)
        self.assertEqual(calls, [])
        with self.assertRaisesRegex(ValueError, 'architecture'):
            module.deploy('host', '/opt/devmesh', 'devmesh:test', run=run)
        self.assertFalse(any('buildx' in args for args in calls))

    def test_failed_local_build_does_not_upload_or_start_remote(self):
        calls, run = self.runner(fail_build=True)
        with self.assertRaises(subprocess.CalledProcessError):
            module.deploy('host', '/opt/devmesh', 'devmesh:test', run=run)
        self.assertFalse(any(args[0] == 'scp' or 'docker load' in args[-1] for args in calls))


if __name__ == '__main__':
    unittest.main()
