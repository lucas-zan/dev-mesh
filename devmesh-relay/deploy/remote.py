"""Build locally, transfer the runtime image over SSH, then start remote Compose."""
import argparse
from datetime import datetime, timezone
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent


def remote_platform(host, run):
    result = run(['ssh', host, 'uname -m'], check=True, capture_output=True, text=True)
    architecture = result.stdout.strip()
    platforms = {'x86_64': 'linux/amd64', 'aarch64': 'linux/arm64', 'arm64': 'linux/arm64'}
    if architecture not in platforms:
        raise ValueError(f'Unsupported remote architecture: {architecture}; set PLATFORM explicitly')
    return platforms[architecture]


def publish(host, directory, image, archive, run):
    destination = shlex.quote(directory)
    run(['ssh', host, f'mkdir -p {destination}'], check=True)
    run(['scp', str(archive), str(HERE / 'compose.yaml'), str(HERE / 'manage.py'),
         f'{host}:{directory}/'], check=True)
    command = (f'cd {destination} && docker load -i relay.tar && rm relay.tar && '
               f'python3 manage.py deploy-image --image {shlex.quote(image)}')
    run(['ssh', host, command], check=True)


def deploy(host, directory, image, platform=None, run=subprocess.run):
    if not host:
        raise ValueError('Set REMOTE_HOST to an SSH host, for example user@server')
    if not directory.startswith('/'):
        raise ValueError('REMOTE_DIR must be an absolute remote path')
    platform = platform or remote_platform(host, run)
    run(['docker', 'buildx', 'build', '--platform', platform, '--load', '-t', image,
         '-f', str(HERE / 'Dockerfile'), str(ROOT)], check=True)
    with tempfile.TemporaryDirectory(prefix='devmesh-publish-') as temporary:
        archive = Path(temporary) / 'relay.tar'
        run(['docker', 'image', 'save', '-o', str(archive), image], check=True)
        publish(host, directory, image, archive, run)
    print(f'Deployed {image} to {host}:{directory}; see remote .env for the Web port.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', default=os.environ.get('REMOTE_HOST', ''))
    parser.add_argument('--directory', default=os.environ.get('REMOTE_DIR', '/opt/devmesh'))
    parser.add_argument('--platform', default=os.environ.get('PLATFORM') or None)
    stamp = datetime.now(timezone.utc).strftime('%Y%m%d%H%M%S')
    parser.add_argument('--image', default=os.environ.get('RELAY_IMAGE') or f'devmesh-relay:{stamp}')
    args = parser.parse_args()
    deploy(args.host, args.directory, args.image, args.platform)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error))
