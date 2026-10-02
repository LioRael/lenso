"""Process cleanup regression: a child that ignores TERM must not survive."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

RUNNER = Path(__file__).with_name("smoke.py")
CHILD = """
import os, pathlib, signal, sys, time
signal.signal(signal.SIGTERM, signal.SIG_IGN)
path = pathlib.Path(sys.argv[1])
while True:
    with path.open('ab') as output:
        output.write(b'x')
    time.sleep(0.02)
"""
LEADER = """
import os, pathlib, subprocess, sys, time
pathlib.Path(sys.argv[2]).write_text(str(os.getpid()))
subprocess.Popen([sys.executable, '-c', sys.argv[3], sys.argv[1]])
time.sleep(60)
"""
WRAPPER = """
import os, runpy, sys
runner = runpy.run_path(sys.argv[1])
runner['run']([sys.executable, '-c', sys.argv[2], *sys.argv[4:]],
              float(sys.argv[3]), dict(os.environ))
"""


@unittest.skipUnless(os.name == "posix", "the runner supports POSIX process groups")
class CleanupTests(unittest.TestCase):
    def exercise(self, interrupt):
        with tempfile.TemporaryDirectory() as directory:
            heartbeat = Path(directory) / "heartbeat"
            group = Path(directory) / "group"
            process = subprocess.Popen(
                [sys.executable, "-c", WRAPPER, str(RUNNER), LEADER,
                 "30" if interrupt else "1", str(heartbeat), str(group), CHILD],
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
            )
            try:
                deadline = time.monotonic() + 5
                while not heartbeat.exists():
                    self.assertIsNone(process.poll(), "fixture exited before readiness")
                    self.assertLess(time.monotonic(), deadline, "fixture never became ready")
                    time.sleep(0.02)
                if interrupt:
                    process.send_signal(signal.SIGINT)
                output, _ = process.communicate(timeout=8)
                self.assertEqual(process.returncode, 130 if interrupt else 124, output)
                before = heartbeat.read_bytes()
                time.sleep(0.15)
                self.assertEqual(heartbeat.read_bytes(), before, "descendant survived cleanup")
            finally:
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=5)
                if group.exists():
                    try:
                        os.killpg(int(group.read_text()), signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    def test_timeout_cleans_term_resistant_descendant(self):
        self.exercise(interrupt=False)

    def test_interrupt_cleans_term_resistant_descendant(self):
        self.exercise(interrupt=True)


if __name__ == "__main__":
    unittest.main()
