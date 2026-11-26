"""Compose deployment for the single Relay/Web service and PostgreSQL."""
import argparse
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import urllib.error
import urllib.request

COMPOSE=Path(__file__).resolve().with_name('compose.yaml')

def environment(path):
    if not path.exists():
        path.parent.mkdir(parents=True,exist_ok=True)
        values={'POSTGRES_PASSWORD':os.environ.get('POSTGRES_PASSWORD') or secrets.token_urlsafe(18),
                'DEVMESH_INITIAL_PASSWORD':os.environ.get('DEVMESH_INITIAL_PASSWORD') or secrets.token_urlsafe(18),
                'DEVMESH_ADMIN_USER':os.environ.get('DEVMESH_ADMIN_USER','admin'),
                'RELAY_BIND_ADDRESS':os.environ.get('RELAY_BIND_ADDRESS','0.0.0.0'),
                'RELAY_PORT':os.environ.get('RELAY_PORT','17843')}
        path.write_text(''.join(f'{key}={value}\n' for key,value in values.items()))
    return {key:value.strip().strip('"').strip("'") for line in path.read_text().splitlines()
            if line.strip() and not line.lstrip().startswith('#') for key,_,value in [line.partition('=')]}

def compose(path,build=False):
    command=['docker','compose','--project-name','devmesh','--env-file',str(path.resolve()),'-f',str(COMPOSE)]
    return command+(['-f',str(COMPOSE.with_name('compose.build.yaml'))] if build else [])

def execute(args,values,capture=False):
    return subprocess.run(args,env={**os.environ,**values},text=True,
                          capture_output=capture,check=not capture)

def wait_health(url):
    for attempt in range(30):
        try:
            with urllib.request.urlopen(url,timeout=2) as response:
                if response.status==200:return
        except (OSError,urllib.error.URLError):
            pass
        if attempt<29:time.sleep(1)
    raise RuntimeError('Relay health check failed; run make logs')

def deploy(path,run=execute,health=wait_health,image=None):
    values=environment(path)
    if image:
        lines=[line for line in path.read_text().splitlines() if not line.startswith('RELAY_IMAGE=')]
        path.write_text('\n'.join(lines)+f'\nRELAY_IMAGE={image}\n')
        values['RELAY_IMAGE']=image
    command=compose(path,build=image is None)
    run(command+['up','--build' if image is None else '--no-build','-d','--wait'],values)
    port=values.get('RELAY_PORT','17843')
    health(f'http://127.0.0.1:{port}/healthz')
    result=run(command+['exec','-T','devmesh-relay','devmesh-relay','bootstrap','--username',values.get('DEVMESH_ADMIN_USER','admin')],values,True)
    credentials=path.parent/'.local/bootstrap.json'
    if result.returncode==0:
        data=json.loads(result.stdout)
        credentials.parent.mkdir(parents=True,exist_ok=True)
        credentials.write_text(json.dumps(data,indent=2)+'\n')
        print(f'Initial user: {values.get("DEVMESH_ADMIN_USER","admin")}')
        print(f'Initial password: {values["DEVMESH_INITIAL_PASSWORD"]}')
    else:
        try:code=json.loads(result.stderr)['error']['code']
        except (ValueError,KeyError,TypeError):raise RuntimeError(result.stderr.strip() or 'Bootstrap failed')
        if code!='already_initialized':raise RuntimeError(result.stderr.strip())
        print('Existing account/database retained.')
    print(f'Deployed Relay/Web: http://localhost:{port} (remote: use this machine\'s address)')
    print(f'Initial device key record: {credentials}')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=['deploy','deploy-image','status','logs','restart','down','config','credentials'])
    parser.add_argument('--env-file',type=Path,default=COMPOSE.with_name('.env'))
    parser.add_argument('--image',help='Loaded image for deploy-image')
    args=parser.parse_args();path=args.env_file.resolve()
    if args.action=='deploy':deploy(path);return
    if args.action=='deploy-image':
        if not args.image:parser.error('deploy-image requires --image')
        deploy(path,image=args.image);return
    if not path.exists():raise RuntimeError('Deployment environment missing; run make local first')
    values=environment(path)
    if args.action=='credentials':
        print(f'Username: {values.get("DEVMESH_ADMIN_USER","admin")}\nPassword: {values["DEVMESH_INITIAL_PASSWORD"]}')
        record=path.parent/'.local/bootstrap.json'
        if record.exists():print(record.read_text())
        return
    commands={'status':['ps'],'logs':['logs','-f','--tail','100','devmesh-relay'],
              'restart':['restart','devmesh-relay'],'down':['down'],'config':['config','--quiet']}
    execute(compose(path)+commands[args.action],values)

if __name__=='__main__':
    try:main()
    except (RuntimeError,OSError,subprocess.CalledProcessError) as error:
        raise SystemExit(str(error))
