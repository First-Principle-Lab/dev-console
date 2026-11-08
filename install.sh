#!/bin/sh
# Standalone installer. Keep the entire Python program in this file so curl | sh
# works without a source checkout. Never run the downloaded AppImage during setup.
set -eu
command -v python3 >/dev/null 2>&1 || {
    echo 'Dev Console: python3 is required. On Arch: sudo pacman -S python' >&2
    exit 1
}
python3 - "$@" <<'PY'
import base64
import fcntl
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

REPO = 'First-Principle-Lab/dev-console'
RELEASES = f'https://github.com/{REPO}/releases'
PUBLIC_KEY = 'RWQSvSwBbnGONng1XGBPEaIxY6T25i0WiUlzv4GakJYD3hUKpRdWmHC4'
MARKER = '# Installed by the Dev Console AppImage installer'


def fail(message):
    raise RuntimeError(message)


def download(url, target):
    subprocess.run([
        'curl', '--fail', '--silent', '--show-error', '--location',
        '--proto', '=https', '--proto-redir', '=https', '--tlsv1.2',
        '--connect-timeout', '15', '--max-time', '600', '--retry', '2',
        '--output', str(target), url,
    ], check=True)


def desktop_quote(value):
    # Exec is parsed by the desktop launcher, not a shell. Escape field codes
    # as well as the reserved characters inside a double-quoted argument.
    value = value.replace('%', '%%')
    for char in ('\\', '"', '`', '$'):
        value = value.replace(char, '\\' + char)
    # Desktop Entry string unescaping happens before Exec argument unquoting.
    return '"' + value.replace('\\', '\\\\') + '"'


def atomic_write(target, content, mode):
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as stream:
        partial = Path(stream.name)
        try:
            stream.write(content)
            stream.flush()
            os.fchmod(stream.fileno(), mode)
            os.replace(partial, target)
        finally:
            partial.unlink(missing_ok=True)


def main(args):
    if args == ['--help']:
        print('Usage: sh install.sh [--with-auth-helper]')
        print('Installs the latest signed Linux x86_64 AppImage for the current user.')
        print('--with-auth-helper also installs the system polkit helper using sudo.')
        return
    if args not in ([], ['--with-auth-helper']):
        fail('Usage: sh install.sh [--with-auth-helper]')
    with_auth = bool(args)
    if platform.system() != 'Linux' or platform.machine() not in ('x86_64', 'amd64'):
        fail('Published AppImages currently support Linux x86_64 only.')
    if os.geteuid() == 0:
        fail('Run this installer as your normal user, without sudo. Use --with-auth-helper for system integration.')
    for tool in ('curl', 'minisign') + (('sudo', 'install') if with_auth else ()):
        if not shutil.which(tool):
            fail(f'{tool} is required. On Arch, install curl, python and minisign with pacman.')
    home = Path(os.environ['HOME'])
    data = Path(os.environ.get('XDG_DATA_HOME') or home / '.local/share')
    for path in (home, data):
        if not path.is_absolute() or any(ord(c) < 32 for c in str(path)):
            fail('HOME and XDG_DATA_HOME must be absolute paths without control characters.')
    if '=' in str(home):
        fail('The Desktop Entry specification does not support = in an executable path.')
    app_dir = data / 'dev-console/app'
    app = app_dir / 'Dev-Console.AppImage'
    launcher = home / '.local/bin/dev-console'
    desktop = data / 'applications/dev-console.desktop'
    icon = data / 'icons/hicolor/128x128/apps/dev-console.png'
    # Do not replace unrelated commands or a managed application's symlink.
    for path in (app, launcher, desktop, icon):
        if path.is_symlink():
            fail(f'Refusing to replace a symlink: {path}')
    if launcher.exists() and MARKER not in launcher.read_text():
        fail(f'{launcher} already exists and was not created by this installer.')
    app_dir.mkdir(parents=True, exist_ok=True)
    with (app_dir / '.install.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            fail('Another Dev Console installation is running.')
        with tempfile.TemporaryDirectory(prefix='.download-', dir=app_dir) as work:
            stage = Path(work)
            manifest = stage / 'latest.json'
            download(f'{RELEASES}/latest/download/latest.json', manifest)
            version = json.loads(manifest.read_text())['version']
            if not isinstance(version, str) or not re.fullmatch(r'\d+\.\d+\.\d+', version):
                fail('The release manifest has an invalid version.')
            # Pin ALL assets to the version we just discovered. Never combine a
            # signature and binary from different releases if Latest changes.
            base = f'{RELEASES}/download/v{version}'
            binary = stage / 'Dev-Console.AppImage'
            encoded_sig = stage / 'signature.base64'
            signature = stage / 'signature.minisig'
            print(f'Downloading Dev Console {version}...', flush=True)
            download(f'{base}/Dev-Console.AppImage', binary)
            download(f'{base}/Dev-Console.AppImage.sig', encoded_sig)
            signature.write_bytes(base64.b64decode(encoded_sig.read_bytes().strip(), validate=True))
            subprocess.run(['minisign', '-V', '-q', '-P', PUBLIC_KEY,
                            '-m', str(binary), '-x', str(signature)], check=True)
            if f'version:{version}' not in signature.read_text().splitlines()[2].split('\t'):
                fail('The signed version does not match the release manifest.')
            with binary.open('rb') as stream:
                if stream.read(4) != b'\x7fELF':
                    fail('The downloaded artifact is not a Linux executable.')
            source = f'https://raw.githubusercontent.com/{REPO}/v{version}'
            staged_icon = stage / 'icon.png'
            download(f'{source}/src-tauri/icons/128x128.png', staged_icon)
            if not staged_icon.read_bytes().startswith(b'\x89PNG\r\n\x1a\n'):
                fail('The downloaded launcher icon is not a PNG.')
            if with_auth:
                helper = stage / 'dev-console-auth-helper'
                policy = stage / 'ai.codemate.devconsole.policy'
                download(f'{source}/packaging/dev-console-auth-helper', helper)
                download(f'{source}/packaging/polkit/ai.codemate.devconsole.policy', policy)
                # Only these two fixed system paths need administrator access.
                # sudo reads its password from the terminal, not the curl pipe.
                subprocess.run(['sudo', 'install', '-Dm755', str(helper),
                                '/usr/libexec/dev-console-auth-helper'], check=True)
                subprocess.run(['sudo', 'install', '-Dm644', str(policy),
                                '/usr/share/polkit-1/actions/ai.codemate.devconsole.policy'], check=True)
            wrapper = f'#!/bin/sh\n{MARKER}\nexec {shlex.quote(str(app))} "$@"\n'
            entry = (
                '[Desktop Entry]\nType=Application\nName=Dev Console\n'
                'Comment=Saved commands and secrets, searchable from anywhere\n'
                f'Exec={desktop_quote(str(launcher))} --library\n'
                'Icon=dev-console\nTerminal=false\nCategories=Utility;\n'
                'Keywords=commands;snippets;secrets;clipboard;terminal;\n'
                'StartupNotify=true\nStartupWMClass=dev-console\nActions=finder;\n'
                '\n[Desktop Action finder]\nName=Open the finder\n'
                f'Exec={desktop_quote(str(launcher))} --finder\n'
            )
            # All downloads and signature verification finish before replacing
            # the installed application. Rename beside the destination atomically.
            binary.chmod(0o755)
            os.replace(binary, app)
            atomic_write(launcher, wrapper.encode(), 0o755)
            atomic_write(icon, staged_icon.read_bytes(), 0o644)
            atomic_write(desktop, entry.encode(), 0o644)
    cache = shutil.which('update-desktop-database')
    if cache:
        subprocess.run([cache, str(desktop.parent)], check=False)
    print(f'Installed Dev Console {version}. Open it from your application launcher or run:')
    print(f'  {shlex.quote(str(launcher))}')
    if str(launcher.parent) not in os.environ.get('PATH', '').split(os.pathsep):
        print('To use dev-console in this shell, add ~/.local/bin to PATH:')
        print('  export PATH="$HOME/.local/bin:$PATH"')
    if not with_auth:
        helper_present = any(Path(prefix + '/libexec/dev-console-auth-helper').is_file()
                             for prefix in ('/usr', '/usr/local'))
        policy_present = any(Path(prefix + '/share/polkit-1/actions/ai.codemate.devconsole.policy').is_file()
                             for prefix in ('/usr', '/usr/local'))
        if helper_present and policy_present:
            print('The system helper and policy for authenticated vault unlock are already installed.')
        else:
            print('Authenticated vault unlock needs the system helper. Rerun with --with-auth-helper to install it.')
    print('Clipboard support needs wl-clipboard on Wayland or xclip on X11; secrets need a running Secret Service provider.')
    print('AppImage updates remain available in the app. Rerunning this installer also downloads the latest release.')
    print('If Dev Console is already running, quit and reopen it to use the installed version.')


if __name__ == '__main__':
    try:
        main(sys.argv[1:])
    except (RuntimeError, OSError, ValueError, KeyError, IndexError, subprocess.CalledProcessError) as error:
        print(f'Dev Console installation failed: {error}', file=sys.stderr)
        sys.exit(1)
PY
