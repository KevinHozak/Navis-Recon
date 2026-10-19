"""Exercise the real publisher in disposable roots; fake only Windows shortcuts.

Run with: python scripts/test_publish_desktop.py
No builds, ledger reads, or repository release artifacts are touched.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from contextlib import contextmanager
from uuid import uuid4


SCRIPT = Path(__file__).with_name('publish-desktop.ps1')
SHELL = shutil.which('powershell.exe')


@contextmanager
def fixture_root():
    # Normal directory permissions also work under Windows sandbox identities.
    root = Path(tempfile.gettempdir()) / ('recon publish ' + uuid4().hex)
    root.mkdir()
    try:
        yield root
    finally:
        shutil.rmtree(root)


@unittest.skipUnless(SHELL, 'Windows PowerShell is required')
class PublisherTests(unittest.TestCase):
    def exercise(self, installers, *, executable=True):
        with fixture_root() as folder:
            root = Path(folder)
            (root / 'scripts').mkdir()
            shutil.copyfile(SCRIPT, root / 'scripts/publish-desktop.ps1')
            release = root / 'frontend/src-tauri/target/release'
            nsis = release / 'bundle/nsis'
            nsis.mkdir(parents=True)
            (root / 'frontend/src-tauri/tauri.conf.json').write_text(
                json.dumps({'productName': 'Navis-Recon', 'version': '1.2.3'}))
            if executable:
                (release / 'navis-recon.exe').write_bytes(b'new executable')
            for index, name in enumerate(installers):
                path = nsis / name
                path.write_bytes(name.encode())
                os.utime(path, (1700000000 + index, 1700000000 + index))
            (root / 'Navis-Recon.exe').write_bytes(b'previous executable')
            (root / 'Navis-Recon-Setup.exe').write_bytes(b'previous installer')
            (root / 'unrelated.txt').write_bytes(b'preserve me')
            # Keep shortcut behavior observable without creating real COM shortcuts.
            wrapper = root / 'run.ps1'
            wrapper.write_text('''
function New-Object {
    param([string]$ComObject)
    if ($ComObject -ne 'WScript.Shell') { throw 'Unexpected COM request' }
    $shell = [pscustomobject]@{}
    $shell | Add-Member ScriptMethod CreateShortcut {
        param($path)
        $link = [pscustomobject]@{Path=$path; TargetPath=''; WorkingDirectory=''; Description=''; IconLocation=''}
        $link | Add-Member ScriptMethod Save { [IO.File]::WriteAllText($this.Path, $this.TargetPath) }
        return $link
    }
    return $shell
}
& "$PSScriptRoot/scripts/publish-desktop.ps1"
''')
            result = subprocess.run([SHELL, '-NoProfile', '-ExecutionPolicy', 'Bypass',
                                     '-File', str(wrapper)], capture_output=True, timeout=30)
            files = {p.name: p.read_bytes() for p in root.iterdir() if p.is_file()}
            self.assertEqual(files['unrelated.txt'], b'preserve me')
            return result, files

    def assert_rejected(self, installers, **kwargs):
        result, files = self.exercise(installers, **kwargs)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(files['Navis-Recon.exe'], b'previous executable')
        self.assertEqual(files['Navis-Recon-Setup.exe'], b'previous installer')
        self.assertNotIn('Navis-Recon.lnk', files)

    def test_missing_installer_preserves_previous_release(self):
        self.assert_rejected([])

    def test_wrong_version_is_rejected(self):
        self.assert_rejected(['Navis-Recon_1.2.2_x64-setup.exe'])

    def test_matching_version_wins_over_newer_timestamp(self):
        expected = 'Navis-Recon_1.2.3_x64-setup.exe'
        result, files = self.exercise([expected, 'Navis-Recon_1.2.2_x64-setup.exe'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(files['Navis-Recon-Setup.exe'], expected.encode())

    def test_normal_release_and_shortcut(self):
        result, files = self.exercise(['Navis-Recon_1.2.3_x64-setup.exe'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(files['Navis-Recon.exe'], b'new executable')
        self.assertTrue(files['Navis-Recon.lnk'].endswith(b'Navis-Recon.exe'))

    def test_missing_executable_preserves_previous_release(self):
        self.assert_rejected(['Navis-Recon_1.2.3_x64-setup.exe'], executable=False)

    def test_held_out_ambiguous_architectures_are_rejected(self):
        self.assert_rejected(['Navis-Recon_1.2.3_x64-setup.exe',
                              'Navis-Recon_1.2.3_arm64-setup.exe'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
