#!/usr/bin/env python3
"""Hermetic wrapper tests: no builds, listeners or package downloads required."""
import json, os, shutil, subprocess, tempfile, unittest
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]

class Wrappers(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='aurora wrapper space ')
        self.root = Path(self.tmp.name)
        for name in ['dev.sh','run.sh']: shutil.copy2(ROOT/name, self.root/name)
        launcher = self.root/'aurora'
        launcher.write_text('#!/usr/bin/env python3\nimport json,os,sys\nprint(json.dumps(sys.argv[1:]))\nsys.exit(int(os.environ.get("FAKE_EXIT","0")))\n')
        launcher.chmod(0o755)
        build = self.root/'build-dist.sh'
        build.write_text('#!/bin/sh\nprintf built > "$(dirname "$0")/built"\nexit "${FAKE_BUILD_EXIT:-0}"\n')
        build.chmod(0o755)
        self.env = {k:v for k,v in os.environ.items() if not k.startswith('AURORA_')}
    def tearDown(self): self.tmp.cleanup()
    def call(self,script,*args,**extra):
        return subprocess.run([str(self.root/script),*args],cwd='/',env={**self.env,**extra},capture_output=True,text=True,timeout=10)
    def args(self,r):
        self.assertEqual(r.returncode,0,r.stderr);return json.loads(r.stdout)
    def test_argument_boundaries_preserved(self):
        a=['--port','11220','--headless','yes','--passwd','spaces $quote ; kept','--launcher','echo --display :bad']
        self.assertEqual(self.args(self.call('run.sh',*a)),a)
    def test_option_looking_password_preserved(self):
        a=['--port','11220','--headless','--passwd','--port']
        self.assertEqual(self.args(self.call('run.sh',*a)),a)
    def test_leading_zero_port_is_forwarded(self):
        a=['--port','09990','--headless=yes'];self.assertEqual(self.args(self.call('run.sh',*a)),a)
    def test_equals_and_override_preserved(self):
        a=['--port=11220','--headless=yes','--display=:321'];self.assertEqual(self.args(self.call('run.sh',*a)),a)
    def test_dev_default_port(self):
        self.assertEqual(self.args(self.call('dev.sh','--headless',AURORA_DEV_SKIP_BUILD='1')),['--port','9990','--headless'])
    def test_dev_user_port_is_last(self):
        self.assertEqual(self.args(self.call('dev.sh','--port=11220','--headless',AURORA_DEV_SKIP_BUILD='1')),['--port','9990','--port=11220','--headless'])
    def test_dev_env_default(self):
        self.assertEqual(self.args(self.call('dev.sh','--headless',AURORA_DEV_PORT='09990',AURORA_DEV_SKIP_BUILD='1')),['--port','09990','--headless'])
    def test_dev_does_not_scan_password(self):
        self.assertEqual(self.args(self.call('dev.sh','--passwd','--port',AURORA_DEV_SKIP_BUILD='1')),['--port','9990','--passwd','--port'])
    def test_dev_build_once(self):
        self.args(self.call('dev.sh','--headless'));self.assertEqual((self.root/'built').read_text(),'built')
    def test_dev_build_error_propagates(self):
        r=self.call('dev.sh','--headless',FAKE_BUILD_EXIT='7');self.assertEqual(r.returncode,7);self.assertEqual(r.stdout,'')
    def test_help_never_builds(self):
        (self.root/'aurora').unlink()
        for script in ['dev.sh','run.sh']:
            r=self.call(script,'--help');self.assertEqual(r.returncode,0,r.stderr);self.assertIn('11220',r.stdout)
        self.assertFalse((self.root/'built').exists())
    def test_version_never_builds(self):
        self.assertEqual(self.args(self.call('dev.sh','--version')),['--version']);self.assertFalse((self.root/'built').exists())
    def test_invalid_dev_environment_rejected(self):
        for text in ['0','65536','x','-1']:
            self.assertEqual(self.call('dev.sh',AURORA_DEV_PORT=text,AURORA_DEV_SKIP_BUILD='1').returncode,2)
        self.assertFalse((self.root/'built').exists())
    def test_bad_skip_flag_rejected(self):
        self.assertEqual(self.call('dev.sh',AURORA_DEV_SKIP_BUILD='yes').returncode,2)
    def test_exit_status_forwarded(self):
        self.assertEqual(self.call('run.sh',FAKE_EXIT='17').returncode,17)
    def test_explicit_binary_and_missing_binary(self):
        override=self.root/'custom executable';shutil.copy2(self.root/'aurora',override)
        self.assertEqual(self.args(self.call('run.sh','--version',AURORA_LAUNCHER_BIN=str(override))),['--version'])
        self.assertEqual(self.call('run.sh',AURORA_LAUNCHER_BIN=str(self.root)).returncode,1)
        (self.root/'aurora').unlink();self.assertEqual(self.call('run.sh').returncode,1)
    def test_source_tree_release_lookup(self):
        dest=self.root/'.output/dist/current';dest.mkdir(parents=True)
        (self.root/'aurora').rename(dest/'aurora')
        self.assertEqual(self.args(self.call('run.sh','--version')),['--version'])

if __name__=='__main__':unittest.main(verbosity=2)
