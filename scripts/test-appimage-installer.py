#!/usr/bin/env python3
"""Exercise the standalone installer with signed fixtures and an isolated HOME."""
import base64
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = (ROOT / 'install.sh').read_text()
PROGRAM = SCRIPT.split("<<'PY'\n", 1)[1].rsplit('\nPY\n', 1)[0]
MODULE = types.ModuleType('appimage_installer')
exec(compile(PROGRAM, str(ROOT / 'install.sh'), 'exec'), MODULE.__dict__)


class InstallerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not shutil.which('minisign'):
            raise RuntimeError('minisign is required to test real signature verification')

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='dev-console-installer-test-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.home = self.base / "demo user's home"
        self.data = self.base / 'data directory'
        self.home.mkdir()
        self.assets = self.base / 'assets'
        self.assets.mkdir()
        self.bin = self.base / 'bin'
        self.bin.mkdir()
        self.log = self.base / 'requests'
        self.log.touch()
        self.env = {**os.environ, 'HOME': str(self.home), 'XDG_DATA_HOME': str(self.data),
                    'PATH': f'{self.bin}:{os.environ["PATH"]}', 'FIXTURES': str(self.assets),
                    'REQUEST_LOG': str(self.log)}
        self.version = '1.2.3'
        (self.assets / 'latest.json').write_text(json.dumps({'version': self.version}))
        self.binary = self.assets / 'Dev-Console.AppImage'
        self.binary.write_bytes(b'\x7fELF synthetic AppImage fixture, never executed')
        self.public = self.base / 'public.key'
        self.secret = self.base / 'secret.key'
        subprocess.run(['minisign', '-G', '-W', '-p', str(self.public), '-s', str(self.secret)],
                       check=True, capture_output=True)
        self.key = self.public.read_text().splitlines()[1]
        self.sign()
        shutil.copyfile(ROOT / 'src-tauri/icons/128x128.png', self.assets / '128x128.png')
        shutil.copyfile(ROOT / 'packaging/dev-console-auth-helper', self.assets / 'dev-console-auth-helper')
        shutil.copyfile(ROOT / 'packaging/polkit/ai.codemate.devconsole.policy', self.assets / 'ai.codemate.devconsole.policy')
        self.installer = self.base / 'install.sh'
        self.installer.write_text(SCRIPT.replace(MODULE.PUBLIC_KEY, self.key))
        self.command('curl', '''import os, pathlib, sys
url = sys.argv[-1]
with open(os.environ['REQUEST_LOG'], 'a') as stream:
    stream.write(url + '\\n')
if os.environ.get('FAIL_DOWNLOAD') in (url.rsplit('/', 1)[-1], '*'):
    sys.exit(22)
if '/latest/download/' not in url and '/v1.2.3/' not in url:
    sys.exit(22)
source = pathlib.Path(os.environ['FIXTURES']) / url.rsplit('/', 1)[-1]
target = pathlib.Path(sys.argv[sys.argv.index('--output') + 1])
target.write_bytes(source.read_bytes())
''')
        self.command('sudo', '''import os, sys
with open(os.environ['REQUEST_LOG'], 'a') as stream:
    stream.write('SUDO ' + repr(sys.argv[1:]) + '\\n')
sys.exit(int(os.environ.get('FAIL_SUDO', '0')))
''')

    def command(self, name, code):
        executable = self.bin / name
        executable.write_text('#!/usr/bin/env python3\n' + code)
        executable.chmod(0o755)

    def sign(self, version=None):
        signature = self.base / 'signature.minisig'
        subprocess.run(['minisign', '-S', '-s', str(self.secret), '-m', str(self.binary),
                        '-x', str(signature), '-t', f'file:Dev-Console.AppImage\tversion:{version or self.version}'],
                       check=True, capture_output=True)
        (self.assets / 'Dev-Console.AppImage.sig').write_bytes(base64.b64encode(signature.read_bytes()))

    def run_install(self, *args, **env):
        # Feed the script over stdin, exactly as curl | sh does.
        return subprocess.run(['sh', '-s', '--', *args], input=self.installer.read_text(),
                              env={**self.env, **env}, text=True, capture_output=True, timeout=15)

    @property
    def app(self):
        return self.data / 'dev-console/app/Dev-Console.AppImage'

    def test_production_key_matches_the_updater(self):
        config = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())
        public = base64.b64decode(config['plugins']['updater']['pubkey']).decode().splitlines()[1]
        self.assertEqual(MODULE.PUBLIC_KEY, public)

    def test_piped_install_pins_assets_sets_up_launcher_and_can_repeat(self):
        for _ in range(2):
            result = self.run_install()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(self.app.read_bytes(), self.binary.read_bytes())
        self.assertEqual(self.app.stat().st_mode & 0o777, 0o755)
        urls = self.log.read_text().splitlines()
        self.assertEqual(sum('/latest/download/' in u for u in urls), 2)
        self.assertTrue(all('/download/v1.2.3/' in u or '/v1.2.3/' in u
                            for u in urls if '/latest/download/' not in u))
        desktop = self.data / 'applications/dev-console.desktop'
        if shutil.which('desktop-file-validate'):
            subprocess.run(['desktop-file-validate', str(desktop)], check=True)
        self.assertIn('--finder', desktop.read_text())
        # Verify wrapper quoting and argument forwarding without running an AppImage.
        self.app.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
        result = subprocess.run([str(self.home / '.local/bin/dev-console'), '--finder', 'two words'],
                                text=True, capture_output=True, check=True)
        self.assertEqual(result.stdout, '--finder\ntwo words\n')
        self.assertFalse(any(self.app.parent.glob('.download-*')))
        self.assertNotIn('SUDO', self.log.read_text())

    def test_corrupted_download_preserves_existing_installation(self):
        self.assertEqual(self.run_install().returncode, 0)
        old = self.app.read_bytes()
        self.binary.write_bytes(b'\x7fELF corrupted payload')
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.app.read_bytes(), old)
        self.assertFalse(any(self.app.parent.glob('.download-*')))

    def test_network_failure_preserves_existing_installation(self):
        self.assertEqual(self.run_install().returncode, 0)
        old = self.app.read_bytes()
        self.assertNotEqual(self.run_install(FAIL_DOWNLOAD='128x128.png').returncode, 0)
        self.assertEqual(self.app.read_bytes(), old)

    def test_rejects_signed_version_mismatch_and_malformed_manifest(self):
        for version in ('9.9.9', '1.2.30'):
            self.sign(version)
            self.assertNotEqual(self.run_install().returncode, 0)
            self.assertFalse(self.app.exists())
        (self.assets / 'latest.json').write_text('{"version":"../../bad"}')
        self.assertNotEqual(self.run_install().returncode, 0)
        self.assertFalse(self.app.exists())

    def test_refuses_foreign_launcher_and_symlinks(self):
        launcher = self.home / '.local/bin/dev-console'
        launcher.parent.mkdir(parents=True)
        launcher.write_text('unrelated command')
        self.assertNotEqual(self.run_install().returncode, 0)
        self.assertEqual(launcher.read_text(), 'unrelated command')
        launcher.unlink()
        launcher.symlink_to(self.base / 'unrelated')
        self.assertNotEqual(self.run_install().returncode, 0)
        self.assertEqual(self.log.read_text(), '')

    def test_auth_helper_is_explicit_and_sudo_failure_keeps_old_app(self):
        self.assertEqual(self.run_install('--with-auth-helper').returncode, 0)
        self.assertEqual(self.log.read_text().count('SUDO '), 2)
        self.assertIn('/usr/libexec/dev-console-auth-helper', self.log.read_text())
        self.assertIn('/usr/share/polkit-1/actions/ai.codemate.devconsole.policy', self.log.read_text())
        old = self.app.read_bytes()
        self.assertNotEqual(self.run_install('--with-auth-helper', FAIL_SUDO='1').returncode, 0)
        self.assertEqual(self.app.read_bytes(), old)

    def test_rejects_unsupported_platform_and_root_before_downloads(self):
        for system, arch, uid in [('Darwin', 'x86_64', 1000), ('Linux', 'aarch64', 1000), ('Linux', 'x86_64', 0)]:
            with patch.object(MODULE.platform, 'system', return_value=system), \
                 patch.object(MODULE.platform, 'machine', return_value=arch), \
                 patch.object(MODULE.os, 'geteuid', return_value=uid), \
                 self.assertRaises(RuntimeError):
                MODULE.main([])

    def test_desktop_argument_escaping(self):
        self.assertEqual(MODULE.desktop_quote('/tmp/a $b'), '"/tmp/a \\\\$b"')
        self.assertEqual(MODULE.desktop_quote('/tmp/50%'), '"/tmp/50%%"')

    def test_concurrent_installer_is_rejected_before_downloading(self):
        self.app.parent.mkdir(parents=True)
        with (self.app.parent / '.install.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Another Dev Console installation', result.stderr)
        self.assertEqual(self.log.read_text(), '')

    def test_xdg_default_and_help(self):
        result = self.run_install(XDG_DATA_HOME='')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.home / '.local/share/dev-console/app/Dev-Console.AppImage').exists())
        self.assertEqual(self.run_install('--help').returncode, 0)
        self.assertNotEqual(self.run_install('--invalid').returncode, 0)


if __name__ == '__main__':
    unittest.main()
