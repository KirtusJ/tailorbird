#!/usr/bin/env python3
"""Model-free actual PCP OS /dev/talk invalid-byte and warning-pixel test.

A pinned opt-in TEST WORKER, not Qwen, sends b"go\xff?" in a real CHUNK.
The acceptance check requires a native QEMU P6, full-frame equality to the
byte-exact source renderer, and negative controls. No trusted PT effect,
installed-VM, physical or permission acceptance is implied.
"""
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

from qemu_smoke import Qemu

if sys.flags.optimize:
    raise SystemExit("Refuse optimized Python: native acceptance uses assertions")

# Independent source-font ink counts pinned in #44's preceding test results.
EXPECTED_INK = {(800, 600): 657, (1920, 1080): 2413}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def ppm(path):
    data = path.read_bytes()
    match = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", data)
    assert match is not None, ("not P6", str(path))
    width, height, maximum = (int(v) for v in match.groups())
    payload = data[match.end():]
    assert maximum == 255 and len(payload) == width * height * 3, "invalid P6 extent"
    return (width, height), payload


def rendered(tool, folder, size, tag, steps):
    sub = folder / ("oracle-" + tag)
    sub.mkdir()
    script = sub / "steps.txt"
    script.write_text("\n".join(steps + ["shot " + tag]) + "\n")
    subprocess.run([str(tool), str(script), str(sub), str(size[0]), str(size[1])], check=True)
    return ppm(sub / (tag + ".ppm"))


def run(kernel, guest, worker, tool, root, size, measurement):
    root.mkdir(parents=True, exist_ok=False)
    for original, name in ((kernel, "kernel.elf32"), (guest, "tailorbird.elf"),
                           (worker, "talk-test-worker")):
        shutil.copyfile(original, root / name)
    guest_pin, worker_pin = digest(guest), digest(worker)
    width, height = size
    line = (f"image={measurement} auto=tailorbird.elf then=off desktop={width}x{height} "
            f"propose_image={guest_pin} worker_for={guest_pin} worker_image={worker_pin}")
    report = {"size": f"{width}x{height}", "status": "FAIL_OR_INCOMPLETE",
              "model": "opt-in test worker, not Qwen",
              "guest_sha256": guest_pin, "worker_sha256": worker_pin,
              "kernel_sha256": digest(kernel)}
    q = Qemu(Path("kernel.elf32"), "tailorbird.elf,talk-test-worker",
             root, "q35", line)
    try:
        def until(marker, seconds=60):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline and q.proc.poll() is None:
                if marker in q.serial():
                    return
                time.sleep(0.1)
            raise AssertionError(f"missing native serial marker {marker!r}: {q.serial()[-2500:]}")

        until("Tailorbird first frame presented")
        until("tailorbird: a model service channel is open")
        until("tailorbird: model service: ready")
        dims, baseline, _ = q.screenshot("ready")
        assert dims == size, ("wrong native display mode", dims, size)
        for ch in "raw":
            q.key(ch)
        q.key("ret")
        until("test worker: raw invalid CHUNK sent (67 6f ff 3f)")
        until("tailorbird: reply 1 kept 4 bytes and is complete")
        dims, native_pixels, native_sha = q.screenshot("native-invalid-byte")
        assert dims == size, ("native mode changed", dims)
        # The CI kernel enables companion only, not proposals. Its native banner
        # is Talk only, not Talk + notes; no "notes" command belongs here.
        before = ["folder taylor", "ready", "type raw", "enter", "took"]
        expected_size, expected = rendered(
            tool, root, size, "expected-warning",
            before + ["chunkhex 676fff3f", "done"])
        plain_size, plain = rendered(
            tool, root, size, "missing-warning",
            before + ["chunk go??", "done"])
        wrong_size, wrong = rendered(
            tool, root, size, "wrong-input",
            ["folder taylor", "ready", "type rax", "enter", "took",
             "chunkhex 676fff3f", "done"])
        assert expected_size == plain_size == wrong_size == size
        # The on-device image itself, not merely a hosted reducer, is the positive.
        assert native_pixels == expected, "native warning P6 does not match full expected frame"
        pixels = sum(expected[i:i+3] != plain[i:i+3]
                     for i in range(0, len(expected), 3))
        assert pixels == EXPECTED_INK[size], ("independent font count", pixels)
        assert native_pixels != plain, "no-warning negative accepted"
        assert native_pixels != wrong, "wrong-input negative accepted"
        assert native_pixels != baseline, "stale pre-answer frame accepted"
        q.key("esc")
        status = q.proc.wait(timeout=30)
        assert status == 33, ("guest exit status", status)
        report.update(status="PASS", warning_pixels=pixels, native_ppm_sha256=native_sha,
                      negative_controls=3, guest_exit=status)
        print("NATIVE_INVALID_CHUNK_PASS", json.dumps(report, sort_keys=True), flush=True)
    finally:
        q.stop()
        (root / "result.json").write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
        (root / "serial-copy.log").write_text(q.serial())


def main():
    if len(sys.argv) != 5:
        raise SystemExit(__doc__)
    kernel, guest, worker, output = (Path(p).resolve() for p in sys.argv[1:])
    assert all(p.is_file() and p.stat().st_size > 10000
               for p in (kernel, guest, worker))
    output.mkdir(parents=True, exist_ok=False)
    osroot = Path(__file__).resolve().parents[2] / "pcp-os"
    measure = osroot / "PCP_OS_Rust/crates/pcpos-kernel/tools/measure.py"
    measurement = subprocess.check_output([sys.executable, str(measure),
                                           str(kernel)], text=True).strip()
    guestroot = Path(__file__).resolve().parents[1]
    subprocess.run(["cargo", "build", "--locked", "--offline", "--example", "talk_frames"],
                   cwd=guestroot, check=True)
    tool = guestroot.parent / "target/debug/examples/talk_frames"
    for size in EXPECTED_INK:
        run(kernel, guest, worker, tool, output / f"{size[0]}x{size[1]}", size, measurement)


if __name__ == "__main__":
    main()
