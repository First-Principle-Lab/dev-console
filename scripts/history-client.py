#!/usr/bin/env python3
"""Send a shell command or cwd visit over the private app socket. Never log contents."""
import json
import os
import socket
import sys


def main():
    visit = len(sys.argv) == 3 and sys.argv[2] == '--visit'
    line = '' if visit else sys.stdin.read(20001)
    if not visit and (not line or len(line) > 20000 or line[0].isspace()):
        return
    runtime = os.environ.get('DEV_CONSOLE_RUNTIME_DIR')
    if not runtime:
        xdg = os.environ.get('XDG_RUNTIME_DIR')
        runtime = os.path.join(xdg, 'dev-console') if xdg else f'/tmp/dev-console-{os.getuid()}'
    request = {'command': 'project.visit', 'path': sys.argv[1]} if visit else {
        'command': 'history.record', 'path': sys.argv[1], 'shellLine': line}
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(1)
        client.connect(os.path.join(runtime, 'control.sock'))
        client.sendall(json.dumps(request).encode() + b'\n')
        client.recv(4096)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, IndexError):
        pass
