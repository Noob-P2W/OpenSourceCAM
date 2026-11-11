"""Repository quality gates. Run from any directory: python3 scripts/check.py all."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import os
import subprocess
import threading

ROOT = Path(__file__).resolve().parent.parent

def run(*command, env=None):
    print('+', ' '.join(command), flush=True)
    import tempfile
    with tempfile.TemporaryFile(mode='w+', encoding='utf-8') as log:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        log.seek(0)
        output = log.read()
        print(output, flush=True)
        summary = os.environ.get('GITHUB_STEP_SUMMARY')
        if result.returncode and os.environ.get('GITHUB_ACTIONS'):
            (ROOT / 'dist').mkdir(exist_ok=True)
            (ROOT / 'dist/ci-failure.txt').write_text('Command: ' + ' '.join(command) + '\n' + output[-12000:], encoding='utf-8')
        if result.returncode and summary:
            with open(summary, 'a', encoding='utf-8') as report:
                report.write('\n## Failed quality check\n\n`' + ' '.join(command) + '`\n\n<pre>' + __import__('html').escape(output[-12000:]) + '</pre>\n')
        result.check_returncode()

def web():
    for script in ('app.js', 'help.js', 'vise.js', 'shell.js', 'dialogs.js', 'viewport.js', 'worker.js', 'cad-worker.js', 'sw.js'):
        run('node', '--check', 'web/' + script)
    run('node', 'web/cad-mesh-smoke.mjs')
    run('cargo', 'fmt', '--all', '--', '--check')
    run('cargo', 'clippy', '--locked', '--no-default-features', '--features', 'web', '--all-targets', '--', '-D', 'warnings')
    run('cargo', 'test', '--locked', '--no-default-features', '--features', 'web')
    run('bash', 'scripts/build-web.sh')
    run('python3', 'scripts/package-web.py')
    server = ThreadingHTTPServer(('127.0.0.1', 0), partial(SimpleHTTPRequestHandler, directory=str(ROOT/'dist/pwa')))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = {**os.environ, 'PWA_URL': f'http://127.0.0.1:{server.server_port}'}
    try:
        run('node', 'web/startup-smoke.mjs', env=env)
        run('node', 'web/smoke.mjs', env=env)
        run('node', 'web/cad-smoke.mjs', env=env)
        run('node', 'web/mesh-repair-smoke.mjs', env=env)
        run('node', 'web/layout-smoke.mjs', env=env)
        run('node', 'web/workflow-smoke.mjs', env=env)
        run('node', 'web/fixture-preview-smoke.mjs', env=env)
        run('node', 'web/vise-wizard-smoke.mjs', env=env)
        run('node', 'web/geometry-smoke.mjs', env=env)
        run('node', 'web/viewport-smoke.mjs', env=env)
        run('node', 'web/playback-smoke.mjs', env=env)
        run('node', 'web/moving-tool-smoke.mjs', env=env)
        run('node', 'web/machining-smoke.mjs', env=env)
        run('node', 'web/editor-smoke.mjs', env=env)
        run('node', 'web/cancel-smoke.mjs', env=env)
        run('node', 'web/reliability.mjs', env=env)
        run('node', 'web/performance-smoke.mjs', env=env)
    finally:
        server.shutdown()
        server.server_close()

def native():
    run('cargo', 'fmt', '--all', '--', '--check')
    run('cargo', 'clippy', '--locked', '--all-targets', '--', '-D', 'warnings')
    run('cargo', 'test', '--locked', '--all-targets')
    run('cargo', 'test', '--locked', '--no-default-features')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('target', choices=['native', 'web', 'all'], default='all', nargs='?')
    args = parser.parse_args()
    if args.target in ('native', 'all'):
        native()
    if args.target in ('web', 'all'):
        web()
