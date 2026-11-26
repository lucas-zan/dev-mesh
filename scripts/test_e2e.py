"""Offline integration: real pipes/Git/SDK/framing; no TCP or PostgreSQL claim."""
import argparse
import os
import subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def run(command):
 print('+ '+' '.join(command),flush=True);subprocess.run(command,cwd=ROOT,check=True)
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--postgres',action='store_true');args=parser.parse_args()
 run(['cargo','test','-p','devmesh-relay','--test','end_to_end','--offline'])
 for suite in ['gitea','worker','functional_flow','reconcile','durable','process','bridge']:
  run(['cargo','test','-p','devmesh-agent','--test',suite,'--offline'])
 if args.postgres:
  if not os.environ.get('DEVMESH_TEST_DATABASE_URL'):raise SystemExit('DEVMESH_TEST_DATABASE_URL is required; PostgreSQL was not tested')
  run(['cargo','test','-p','devmesh-relay','--test','postgres','--offline','--','--ignored'])
 print('PASS: in-memory Agent/Relay framing, native subprocess IPC, Git delivery/QA/merge flows and Durable SQLite; Gitea/model responses in functional_flow are test doubles; live acceptance is separate.',flush=True)
