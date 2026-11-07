#!/usr/bin/env python3
"""Exercise the installed zsh hook shape against a disposable real Unix socket."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import unittest


class DirectoryHookTest(unittest.TestCase):
    def test_directory_changes_report_paths_without_commands_or_manual_add(self):
        repo = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch)
            runtime = base / 'runtime'
            runtime.mkdir()
            folders = [base / 'project one', base / 'project-two']
            for folder in folders:
                folder.mkdir()
            hook = base / 'hook.zsh'
            client = str(repo / 'scripts/history-client.py')
            hook.write_text((repo / 'scripts/shell-history.zsh').read_text().replace(
                '__DEV_CONSOLE_CLIENT__', "'" + client.replace("'", "'\"'\"'") + "'"))
            requests = []
            received = threading.Event()
            with socket.socket(socket.AF_UNIX) as server:
                server.bind(str(runtime / 'control.sock'))
                server.listen(10)
                server.settimeout(5)

                def collect():
                    try:
                        while len(requests) < 3:
                            connection, _ = server.accept()
                            with connection:
                                data = b''
                                while not data.endswith(b'\n'):
                                    data += connection.recv(4096)
                                requests.append(json.loads(data))
                                connection.sendall(b'{"ok":true}\n')
                    finally:
                        received.set()

                thread = threading.Thread(target=collect, daemon=True)
                thread.start()
                env = {**os.environ, 'DEV_CONSOLE_RUNTIME_DIR': str(runtime)}
                # Positional arguments preserve paths with spaces and avoid shell interpolation.
                result = subprocess.run(['zsh', '-dfi', '-c',
                    'source "$1"; cd "$2"; cd "$3"', 'hook-test', str(hook),
                    *map(str, folders)], cwd=base, env=env, capture_output=True, timeout=5)
                self.assertEqual(result.returncode, 0)
                self.assertTrue(received.wait(5))
                thread.join(timeout=1)
                self.assertEqual({r['path'] for r in requests}, {str(base), *map(str, folders)})
                self.assertTrue(all(r['command'] == 'project.visit' for r in requests))
                self.assertTrue(all('shellLine' not in r for r in requests))


if __name__ == '__main__':
    unittest.main()
