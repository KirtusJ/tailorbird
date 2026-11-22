"""Optical reading-progress regression in one offline PCP OS launch with the test worker."""
import argparse
import hashlib
from pathlib import Path
import shutil
import time

from qemu_smoke import Qemu
from talk_native import Screens, frames_tool, pixels


def check(kernel, foreground, worker, output):
    output.mkdir(parents=True, exist_ok=False)
    screens = Screens(frames_tool(), output, [])
    sent = ["ready", "type sequence", "enter", "took"]
    stages = [
        ("first", "sequence first 3/7", "x", "3 7", "2 7"),
        ("regression", "sequence regression 2/7", "y", "3 7", "2 7"),
        ("changed-total", "sequence changed total 2/9", "z", "3 7", "2 9"),
        ("advance", "sequence advanced 4/7", "v", "4 7", "3 7"),
    ]
    # Each oracle starts with a fresh reading state, so even the rejected values have
    # a distinct expected picture. An ordinary key proves the guest painted again.
    wanted = {}
    draft = ""
    for name, _, key, count, wrong in stages:
        draft += key
        prefix = sent + ["type " + draft]
        wanted[name] = screens.draw(prefix + ["reading " + count], name)
        alternative = screens.draw(prefix + ["reading " + wrong], name + "-wrong")
        if alternative == wanted[name]:
            raise ValueError("the two oracle pictures are identical: " + name)
    stopped = sent + ["type " + draft, "reading 4 7", "stop", "took"]
    retry = stopped + ["clear", "type again", "enter", "took"]
    wanted["stopped"] = screens.draw(stopped, "stopped")
    wanted["replacement"] = screens.draw(retry + ["reading 1 5"], "replacement")
    wanted["complete"] = screens.draw(retry + ["chunk ok", "done"], "complete")
    if len(set(wanted.values())) != len(wanted):
        raise ValueError("expected distinct full-frame states")

    files = [(kernel, "kernel.elf32"), (foreground, "tailorbird.elf"), (worker, "talk-test-worker")]
    for path, name in files:
        shutil.copyfile(path, output / name)
    front, back = (hashlib.sha256(p.read_bytes()).hexdigest() for p in (foreground, worker))
    boot = f"auto=tailorbird.elf then=off desktop=800x600 worker_for={front} worker_image={back}"
    vm = Qemu(Path("kernel.elf32"), "tailorbird.elf,talk-test-worker", output, "q35", boot)

    def wait_for(marker):
        deadline = time.monotonic() + 45
        while vm.proc.poll() is None and time.monotonic() < deadline:
            if marker in vm.serial():
                return
            time.sleep(0.1)
        raise RuntimeError("missing guest serial marker: " + marker + "\n" + vm.serial()[-1800:])

    def capture(name):
        deadline = time.monotonic() + 2.5
        path = output / (name + ".ppm")
        while time.monotonic() < deadline:
            vm.screenshot(name)
            if pixels(path) == wanted[name]:
                print("Matched full guest display", name, hashlib.sha256(path.read_bytes()).hexdigest(), flush=True)
                return
            time.sleep(0.1)
        raise AssertionError("guest display differs: " + name)

    def submit(text):
        for char in text:
            vm.key(char)
        vm.key("ret")

    try:
        wait_for("tailorbird: model service: ready")
        submit("sequence")
        for name, marker, key, _, _ in stages:
            wait_for("test worker: " + marker)
            vm.key(key)
            capture(name)
        vm.key("alt", "s")
        wait_for("test worker: sequence received matching Stop")
        wait_for("tailorbird: reply 1 kept 0 bytes and is stopped by the person")
        capture("stopped")
        for _ in draft:
            vm.key("backspace")
        submit("again")
        wait_for("test worker: replacement read 1/5")
        capture("replacement")
        wait_for("tailorbird: reply 2 kept 2 bytes and is complete")
        capture("complete")
        vm.key("esc")
        status = vm.proc.wait(timeout=35)
        if status != 33 or "ALL SELF-TESTS PASS" not in vm.serial():
            raise RuntimeError(f"unexpected kernel exit {status}")
    finally:
        vm.stop()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for argument in ("kernel", "foreground", "worker", "output"):
        parser.add_argument(argument, type=Path)
    args = parser.parse_args()
    check(*(value.resolve() for value in (args.kernel, args.foreground, args.worker, args.output)))
