"""Actual environment preflight and an opt-in task run against a dedicated Issue.

Passing this script does not certify browser interaction or native control/recovery.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time


def check(name, status, detail, **fields):
    return dict(name=name, status=status, detail=detail, **fields)


def passed(checks):
    return bool(checks) and all(item['status'] == 'PASS' for item in checks)


def environment_checks():
    results = []
    for name, family in [('tcp', socket.AF_INET), ('unix', socket.AF_UNIX)]:
        try:
            with tempfile.TemporaryDirectory(prefix='devmesh-preflight-') as directory:
                with socket.socket(family) as connection:
                    address = ('127.0.0.1', 0) if name == 'tcp' else str(Path(directory)/'probe.sock')
                    connection.bind(address)
            results.append(check(name, 'PASS', 'listen_available'))
        except OSError:
            results.append(check(name, 'FAIL', 'listen_unavailable'))
    try:
        output = subprocess.run(['ps', '-p', str(os.getpid()), '-o', 'pid='], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=5)
        results.append(check('process', 'PASS' if output.returncode == 0 else 'FAIL', 'process_inspection'))
    except (OSError, subprocess.TimeoutExpired):
        results.append(check('process', 'FAIL', 'process_inspection_unavailable'))
    return results


def doctor_checks(report):
    results = [check('gitea', 'PASS' if report.get('gitea', {}).get('matches') else 'FAIL', 'repository_identity'),
               check('relay', 'PASS' if report.get('relay', {}).get('authenticated') else 'FAIL', 'relay_identity')]
    protocols = report.get('native_protocols', [])
    if not protocols:
        results.append(check('native', 'NOT_RUN', 'no_native_handshake'))
    for native in protocols:
        ok = native.get('handshake') is True and native.get('quiesced') is True
        results.append(check(native['runtime'], 'PASS' if ok else 'FAIL', 'native_handshake_only'))
    return results


def issue_metadata(issue):
    block = issue['body'].split('<!-- devmesh:issue', 1)[1].split('-->', 1)[0]
    return json.loads(block)


def run_issue(execute, number, project, runtime, timeout):
    target = ['--number', str(number), '--project', str(project)]
    initial = issue_metadata(execute(['issue', 'get', *target]))
    if initial['state'] != 'ready':
        return check(runtime, 'FAIL', 'dedicated_issue_must_be_ready')
    execute(['worker', 'start'])
    deadline = time.monotonic() + timeout
    while True:
        try:
            execute(['worker', 'status'])
            break
        except (OSError, ValueError, subprocess.SubprocessError):
            if time.monotonic() >= deadline:
                return check(runtime, 'FAIL', 'daemon_start_timeout')
            time.sleep(1)
    started = execute(['work', *target])
    while time.monotonic() < deadline:
        meta = issue_metadata(execute(['issue', 'get', *target]))
        if meta['state'] in ('review', 'done'):
            return check(runtime, 'PASS', 'real_task_delivery', work_id=started.get('work_id'),
                         issue_number=number, candidate_sha=meta.get('delivery', {}).get('candidate_sha'))
        if meta['state'] in ('blocked', 'rework'):
            return check(runtime, 'FAIL', 'issue_blocked_or_rework', issue_number=number)
        time.sleep(2)
    return check(runtime, 'FAIL', 'task_timeout', issue_number=number)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path)
    parser.add_argument('--prefix', type=Path, default=Path.home()/'.local/share/devmesh')
    parser.add_argument('--cli', type=Path)
    parser.add_argument('--run-issue', type=int, help='Execute this ready Issue in a dedicated test repository')
    parser.add_argument('--project', type=int, default=0)
    parser.add_argument('--timeout', type=int, default=600)
    parser.add_argument('--output', type=Path, default=Path('docs/verification/live.json'))
    args = parser.parse_args()
    results = environment_checks()
    cli = args.cli or args.prefix/'bin/devmesh'
    def execute(command):
        result = subprocess.run([str(cli), *command, '--config', str(args.config), '--prefix', str(args.prefix)],
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=60, check=True)
        return json.loads(result.stdout)
    if args.config is None:
        results.append(check('doctor', 'NOT_RUN', 'config_not_provided'))
    elif not passed(results):
        results.append(check('doctor', 'NOT_RUN', 'execution_environment_unavailable'))
    else:
        try:
            results.extend(doctor_checks(execute(['doctor', '--verify'])))
            if args.run_issue is not None and passed(results):
                config = json.loads(args.config.read_text())
                if any(project['auto_claim'] for project in config['projects']):
                    raise ValueError('Use a dedicated validation config with auto_claim=false')
                results.append(run_issue(execute, args.run_issue, args.project, config['projects'][args.project]['runtime'], args.timeout))
        except (OSError, ValueError, KeyError, IndexError, subprocess.SubprocessError):
            results.append(check('live_execution', 'FAIL', 'cli_or_configuration_failed'))
    report = dict(verified_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  scope='Live preflight' if args.run_issue is None else 'Live preflight and one dedicated model task',
                  checks=results, passed=passed(results),
                  not_covered=['browser_interaction', 'steer_interrupt_approval_resume', 'manual_merge_and_final_reconcile'])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2))
    return 0 if report['passed'] else 2


if __name__ == '__main__':
    raise SystemExit(main())
