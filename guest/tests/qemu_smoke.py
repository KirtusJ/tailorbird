#!/usr/bin/env python3
"""Emulated, model-free Tailorbird guest smoke.

Runs the public PCP OS kernel and public Tailorbird ELF in software QEMU.
Uses only the stdlib, real QMP keyboard events and actual QEMU scanout PPM.
This is NOT a test of the proprietary PT integrated service, approval,
physical hardware, EDID or full runtime authorization.
"""
import argparse
import hashlib
import json
import pathlib
import re
import socket
import subprocess
import tempfile
import time

DEFAULT_TIMEOUT = 100.0


class Qemu:
    def __init__(self, kernel: pathlib.Path, guest: pathlib.Path, folder: pathlib.Path):
        self.folder = folder
        self.qmp_path = folder / "q"
        self.serial_path = folder / "serial.log"
        cmd = [
            "qemu-system-x86_64", "-cpu", "max", "-m", "256M",
            "-kernel", str(kernel), "-initrd", str(guest),
            "-append", "auto=tailorbird then=off",
            "-display", "none", "-vga", "std", "-nic", "none",
            "-serial", "file:" + str(self.serial_path),
            "-no-reboot", "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
            "-qmp", "unix:" + str(self.qmp_path) + ",server=on,wait=off",
        ]
        print("QEMU_ARGS", json.dumps(cmd), flush=True)
        self.proc = subprocess.Popen(
            cmd, cwd=folder, stdout=subprocess.PIPE, stderr=subprocess.STDOUT
        )
        self.sock = socket.socket(socket.AF_UNIX)
        for _ in range(400):
            if self.proc.poll() is not None:
                raise RuntimeError("QEMU stopped before QMP, rc=" + str(self.proc.returncode))
            try:
                self.sock.connect(str(self.qmp_path))
                break
            except OSError:
                time.sleep(0.05)
        else:
            raise TimeoutError("No QMP socket")
        self.sock.settimeout(10)
        self.conn = self.sock.makefile("rwb")
        self._response()
        self.qmp("qmp_capabilities")

    def _response(self):
        for _ in range(30):
            payload = self.conn.readline()
            if not payload:
                raise EOFError("QMP closed")
            item = json.loads(payload)
            if "event" in item:
                continue
            if "error" in item:
                raise RuntimeError("QMP error: " + str(item["error"]))
            return item
        raise RuntimeError("Too many unsolicited QMP events")

    def qmp(self, method, **arguments):
        self.conn.write((json.dumps({"execute": method, "arguments": arguments}) + "\n").encode())
        self.conn.flush()
        return self._response()

    def key(self, *qcodes):
        self.qmp("send-key",
                 keys=[{"type": "qcode", "data": key} for key in qcodes],
                 **{"hold-time": 100})
        time.sleep(0.22)

    def screenshot(self, tag):
        path = self.folder / (tag + ".ppm")
        self.qmp("screendump", filename=str(path))
        buf = path.read_bytes()
        header = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", buf)
        if header is None:
            raise RuntimeError("Unexpected PPM header: " + repr(buf[:80]))
        width, height, maxval = (int(g) for g in header.groups())
        pixels = buf[header.end():]
        if maxval != 255 or len(pixels) != width * height * 3:
            raise RuntimeError("Unsupported PPM payload, len=" + str(len(pixels)))
        return (width, height), pixels, hashlib.sha256(buf).hexdigest()

    def serial(self):
        return self.serial_path.read_text(errors="replace") if self.serial_path.exists() else ""

    def stop(self):
        if self.proc.poll() is None:
            self.proc.kill()
        self.proc.wait(timeout=10)
        self.conn.close()
        self.sock.close()
        if self.proc.stdout:
            extra = self.proc.stdout.read().decode(errors="replace")
            if extra:
                print("QEMU_STDOUT", extra[-4000:], flush=True)


def crop(pixels: bytes, width: int, box):
    left, top, right, bottom = box
    return b"".join(
        pixels[(y * width + left) * 3:(y * width + right) * 3]
        for y in range(top, bottom)
    )


def run(kernel, guest, timeout):
    assert kernel.is_file() and kernel.stat().st_size > 10000
    assert guest.is_file() and guest.stat().st_size > 10000
    with tempfile.TemporaryDirectory(prefix="tailorbird-qmp-") as scratch:
        folder = pathlib.Path(scratch)
        m = Qemu(kernel, guest, folder)
        try:
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if m.proc.poll() is not None:
                    raise RuntimeError("QEMU exited before guest display: " + m.serial()[-1800:])
                try:
                    dims, baseline, first_hash = m.screenshot("initial")
                    if dims == (960, 540):
                        break
                except RuntimeError as e:
                    # QMP screen may be transient during mode negotiation.
                    print("SCREEN_PENDING", str(e)[:160], flush=True)
                time.sleep(0.3)
            else:
                raise TimeoutError("No 960x540 guest surface. last_screen=" + str(dims)
                                   + " serial=" + m.serial()[-1800:])

            editor = (140, 460, 920, 500)
            base = crop(baseline, dims[0], editor)
            m.key("a")
            dims_a, pixels_a, hash_a = m.screenshot("lower")
            assert dims_a == dims
            lower = crop(pixels_a, dims[0], editor)
            assert lower != base, "Typing lowercase a did not change request pixels"
            m.key("backspace")
            m.key("shift", "a")
            dims_A, pixels_A, hash_A = m.screenshot("upper")
            assert dims_A == dims
            upper = crop(pixels_A, dims[0], editor)
            assert upper != base and upper != lower, "A and a have identical request pixels"

            # Preserve PT's adverse event: the previous guest reduced
            # z,x,z,Shift+a,a in one raw QMP batch to "azx" because it
            # scanned the collapsed pressed-key bitmap numerically.
            # Compare actual scanout cells after spaced and batched events.
            m.key("backspace")  # remove the uppercase A
            for key in ("z", "x", "z"):
                m.key(key)
            m.key("shift", "a")
            m.key("a")
            dims_ordered, ordered_pixels, hash_ordered = m.screenshot("ordered_spaced")
            assert dims_ordered == dims
            ordered_editor = crop(ordered_pixels, dims[0], editor)
            for _ in range(5):
                m.key("backspace")
            for key in ("a", "z", "x"):
                m.key(key)
            dims_collapsed, collapsed_pixels, hash_collapsed = m.screenshot("old_bad_order")
            assert dims_collapsed == dims
            old_bad_editor = crop(collapsed_pixels, dims[0], editor)
            for _ in range(3):
                m.key("backspace")

            def ev(key, down):
                return {"type": "key", "data": {
                    "down": down, "key": {"type": "qcode", "data": key},
                }}

            def tap(key):
                return [ev(key, True), ev(key, False)]

            batch = []
            batch += tap("z")
            batch += tap("x")
            batch += tap("z")
            batch += [ev("shift", True)]
            batch += tap("a")
            batch += [ev("shift", False)]
            batch += tap("a")
            m.qmp("input-send-event", events=batch)
            time.sleep(0.30)
            dims_burst, burst_pixels, hash_burst = m.screenshot("pt_ordered_burst")
            assert dims_burst == dims
            burst_editor = crop(burst_pixels, dims[0], editor)
            assert burst_editor != old_bad_editor, (
                "PT counterexample: batched input collapsed to azx pixels"
            )
            assert burst_editor == ordered_editor, (
                "PT counterexample: batch did not render zxzAa in original order"
            )
            print("PASS: QMP same-batch zxzAa matches spaced input, unlike old azx",
                  flush=True)

            # The preview is deliberately disconnected. Enter must not start
            # a model service or create a privileged tool result.
            m.key("ret")
            dims_enter, pixels_enter, hash_enter = m.screenshot("submitted_offline")
            assert dims_enter == dims
            assert pixels_enter != pixels_A, "Offline Submit did not change any visible state"

            # The inherited kernel must regain the display and close its
            # own register after Esc, reporting the usual debug-exit status.
            m.key("esc")
            try:
                rc = m.proc.wait(timeout=35)
            except subprocess.TimeoutExpired as exc:
                raise TimeoutError("Guest did not exit after Esc") from exc
            log = m.serial()
            print("SCREEN_SHA256", json.dumps({
                "initial": first_hash, "lowercase": hash_a,
                "uppercase": hash_A, "ordered_spaced": hash_ordered,
                "old_collapsed": hash_collapsed, "pt_burst": hash_burst,
                "offline_enter": hash_enter,
            }), flush=True)
            print("SERIAL_TAIL", log[-2800:], flush=True)
            assert rc == 33, "Expected kernel normal self-test/debug exit 33, got " + str(rc)
            assert "ALL SELF-TESTS PASS" in log, "Kernel self-test result not observed"
            assert "  FAIL" not in log, "Kernel reported failing self-checks"
            print("PASS: QEMU scanout/input contrast, offline refusal and clean kernel return",
                  flush=True)
        finally:
            m.stop()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("kernel", type=pathlib.Path)
    parser.add_argument("guest", type=pathlib.Path)
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT)
    args = parser.parse_args()
    run(args.kernel.resolve(), args.guest.resolve(), args.timeout)
