"""Host-only failure controls; these do not execute or qualify a native guest."""
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import qemu_smoke
import view_scale


class CaptureFailures(unittest.TestCase):
    def test_constructor_failures_leave_explicit_receipts(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            kernel, guest = root / "kernel", root / "tailorbird"
            kernel.write_bytes(b"k" * 12000)
            guest.write_bytes(b"g" * 12000)
            for module in (qemu_smoke, view_scale):
                with self.subTest(module=module.__name__):
                    output = root / module.__name__
                    with patch.object(module, "Qemu", side_effect=RuntimeError("QMP handshake failed")):
                        with self.assertRaisesRegex(RuntimeError, "QMP handshake failed"):
                            if module is qemu_smoke:
                                module.run(kernel, guest, 1, "pc", output)
                            else:
                                module.check(kernel, guest, "test-only", output, False)
                    report = json.loads((output / "result.json").read_text())
                    self.assertIs(report["passed"], False)
                    self.assertEqual(report["error"], "RuntimeError: QMP handshake failed")
                    self.assertIn("kernel_sha256", report)

    def test_failed_handshake_reaps_child_and_closes_pipes(self):
        process = Mock(stdin=io.BytesIO(), stdout=io.BytesIO())
        process.poll.return_value = None
        with patch.object(qemu_smoke.subprocess, "Popen", return_value=process), \
                patch.object(qemu_smoke.Qemu, "_response", side_effect=EOFError("QMP closed")):
            with self.assertRaisesRegex(EOFError, "QMP closed"):
                qemu_smoke.Qemu(Path("kernel"), Path("tailorbird"), Path("captures"), "pc")
        process.kill.assert_called_once()
        process.wait.assert_called_once()
        self.assertTrue(process.stdin.closed)
        self.assertTrue(process.stdout.closed)


if __name__ == "__main__":
    unittest.main()
