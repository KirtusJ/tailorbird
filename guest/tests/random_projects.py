"""Exercise Taylor's native note projects using keyboard input and independent screen readback."""
import argparse
import hashlib
import json
import random
import runpy
import shutil
import subprocess
import sys
import time
from functools import lru_cache
from pathlib import Path

from note_roundtrip import ASK, CONFIRM, exact, panel
from qemu_smoke import Qemu, crop
from view_scale import double_glyphs, kernel_glyphs, stop_cell_rgb

ROOT = Path(__file__).resolve().parents[2]
FOLDER = "taylor/my-first-project"
CHECKED = "Checked: exactly what PCP OS made when you allowed it."
PANEL = bytes.fromhex("18364f")
GROUND = bytes.fromhex("10283f")
TEXT = bytes.fromhex("f2f7fc")
GOOD = bytes.fromhex("8fe0c2")


@lru_cache
def fonts():
    return kernel_glyphs(), double_glyphs()


def caption(frame, text, x, y, ink, background):
    # Native Fit at 800x600 maps 960x540 into the central 800x450 pixels.
    size, pixels, _ = frame
    if size != (800, 600):
        return False
    screen_x = lambda value: value * 800 // 960
    screen_y = lambda value: 75 + value * 800 // 960
    top, bottom = screen_y(y), screen_y(y + 12)
    for col, char in enumerate(text):
        left, right = screen_x(x + 6*col), screen_x(x + 6*(col+1))
        expected = stop_cell_rgb(*fonts(), char, right-left, bottom-top, ink, background)
        if crop(pixels, size[0], (left, top, right, bottom)) != expected:
            return False
    return True


def documents(seed):
    rng = random.Random(seed)
    topics = rng.sample(["orchard", "library", "weather", "studio", "garden", "bakery"], 3)
    return [f"{topic} {rng.choice(['plan', 'list', 'log'])} {rng.randrange(10, 100)}" for topic in topics]


def run(kernel, guest, out, seed):
    out.mkdir(parents=True, exist_ok=False)
    measure = ROOT/"pcp-os/PCP_OS_Rust/crates/pcpos-kernel/tools/measure.py"
    image = subprocess.check_output([sys.executable, measure, kernel], text=True).strip()
    guest_sha = hashlib.sha256(guest.read_bytes()).hexdigest()
    shutil.copyfile(kernel, out/"kernel.elf32")
    shutil.copyfile(guest, out/"tailorbird.elf")
    boot = f"image={image} auto=tailorbird.elf then=off desktop=800x600 propose_image={guest_sha}"
    reader = runpy.run_path(str(ROOT/"pcp-os/examples/tester-cd/check.py"))
    glyphs = reader["kernel_glyphs"]()
    docs = documents(seed)
    (out/"inputs.json").write_text(json.dumps({"seed":seed, "documents":docs, "image":image, "guest":guest_sha}, indent=2))
    vm = Qemu(Path("kernel.elf32"), Path("tailorbird.elf"), out, "q35", boot)

    def until(predicate, description):
        end = time.monotonic() + 35
        while time.monotonic() < end and vm.proc.poll() is None:
            if predicate():
                return
            time.sleep(0.1)
        raise RuntimeError(f"{description}; see {out}")

    def preview(tag, lines):
        def matches():
            vm.screenshot(tag)
            rows = reader["read_screen"]((out/(tag+".ppm")).read_bytes(), glyphs)
            return exact(rows, lines)
        until(matches, tag)

    try:
        until(lambda: "Tailorbird first frame presented" in vm.serial(), "first frame")
        previous = ""
        for number, doc in enumerate(docs, start=1):
            for _ in previous:
                vm.key("backspace")
            for char in doc:
                vm.key("spc" if char == " " else char)
            vm.key("alt", "n")
            name = f"note-{number}.txt"
            preview(f"request-{number}", panel(FOLDER, name, doc, ASK))
            if number == 1:
                vm.key("esc")
                until(lambda: "tailorbird: Not made: you said no." in vm.serial(), "refusal")
            else:
                vm.key("f12")
                preview(f"confirm-{number}", panel(FOLDER, name, doc, CONFIRM))
                vm.key("ret")
                until(lambda: f"tailorbird: Made {FOLDER}/{name}." in vm.serial(), f"create {name}")
            previous = doc
        vm.key("alt", "p")
        vm.key("ret")
        for row, doc in enumerate(docs[1:]):
            if row:
                vm.key("left")
                vm.key("down")
            vm.key("ret")
            name = f"note-{row + 2}.txt"
            def matches_open():
                frame = vm.screenshot("opened-" + name)
                return (caption(frame, f"{FOLDER}/{name} ", 339, 184, TEXT, PANEL)
                        and caption(frame, doc+" ", 343, 231, TEXT, GROUND)
                        and caption(frame, CHECKED, 339, 380, GOOD, PANEL))
            until(matches_open, f"Open must show full path, content and matching receipt for {name}")
        vm.key("esc")
        if vm.proc.wait(timeout=30) != 33:
            raise RuntimeError("unexpected kernel exit")
        serial = vm.serial()
        for number, doc in enumerate(docs, start=1):
            digest = hashlib.sha256((doc+"\n").encode()).hexdigest()
            prefix = f"proposal 1 of attempt {number}: {FOLDER}/note-{number}.txt "
            outcome = "DENIED" if number == 1 else "CREATED"
            if sum(line.startswith(prefix) and digest in line and outcome in line for line in serial.splitlines()) != 1:
                raise RuntimeError(f"missing or duplicate receipt for {number}")
        expected_bytes = sum(len(text)+1 for text in docs[1:])
        if f"the store holds 2 file(s), {expected_bytes} bytes" not in serial:
            raise RuntimeError("unexpected store inventory after refusal and two approvals")
        (out/"result.json").write_text(json.dumps({"seed":seed,"declined":1,"created":2,"reopened_and_checked":2,"stored_bytes":expected_bytes,"exit":33}))
    finally:
        vm.stop()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path)
    parser.add_argument("guest", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--seed", type=int, required=True)
    args = parser.parse_args()
    run(args.kernel.resolve(), args.guest.resolve(), args.output.resolve(), args.seed)
