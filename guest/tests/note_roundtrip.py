"""Decline a note, then approve a second note from the same running Tailorbird.

Requires a proposals-enabled PCP OS kernel and a released guest containing Alt+N.
Run with Python 3, an initialized pcp-os submodule, and qemu-system-x86_64 on PATH.
"""
import argparse
import hashlib
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import time

from qemu_smoke import Qemu


def run(kernel, guest, output):
    output.mkdir(parents=True, exist_ok=False)
    os_root = Path(__file__).resolve().parents[2] / "pcp-os"
    measure = os_root / "PCP_OS_Rust/crates/pcpos-kernel/tools/measure.py"
    image = subprocess.check_output([sys.executable, measure, kernel], text=True).strip()
    guest_hash = hashlib.sha256(guest.read_bytes()).hexdigest()
    # QEMU includes the kernel filename in the Multiboot command. Short local
    # names keep the complete image and proposal digests within its byte limit.
    shutil.copyfile(kernel, output / "kernel.elf32")
    shutil.copyfile(guest, output / "tailorbird.elf")
    command = f"image={image} auto=tailorbird.elf then=off desktop=800x600 propose_image={guest_hash}"
    screen = runpy.run_path(str(os_root / "examples/tester-cd/check.py"))
    glyphs = screen["kernel_glyphs"]()
    document = "garden note"
    digest = hashlib.sha256((document + "\n").encode()).hexdigest()
    folder = "taylor/my-first-project"
    vm = Qemu(Path("kernel.elf32"), Path("tailorbird.elf"), output, "q35", command)
    try:
        def wait_for(check, description):
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline and vm.proc.poll() is None:
                if check():
                    return
                time.sleep(0.1)
            raise RuntimeError(f"{description}; see {output}")

        def visible(tag, required):
            def matches():
                vm.screenshot(tag)
                rows = screen["read_screen"]((output / (tag + ".ppm")).read_bytes(), glyphs)
                words = " ".join(" ".join(rows or []).split())
                return all(part in words for part in required)
            wait_for(matches, tag)

        def preview(number):
            vm.key("alt", "n")
            visible(f"preview-{number}", ["PCP OS ASKS YOU", f"Folder: {folder}",
                                        f"File: note-{number}.txt", document,
                                        "(end of the file)", "Page 1 of 1"])

        wait_for(lambda: "Tailorbird first frame presented" in vm.serial(), "first frame")
        for char in document:
            vm.key("spc" if char == " " else char)
        preview(1)
        vm.key("esc")
        wait_for(lambda: "tailorbird: Not made: you said no." in vm.serial(), "first refusal")
        preview(2)
        vm.key("f12")
        visible("confirm", ["Now press Enter to allow it."])
        vm.key("ret")
        made = f"tailorbird: Made {folder}/note-2.txt. Kept until power-off."
        wait_for(lambda: made in vm.serial(), "second note receipt")
        vm.key("esc")
        status = vm.proc.wait(timeout=30)
        serial = vm.serial()
        expected = [(1, "DENIED"), (2, "CREATED")]
        for number, outcome in expected:
            prefix = f"proposal 1 of attempt {number}: {folder}/note-{number}.txt "
            if sum(line.startswith(prefix) and digest in line and outcome in line
                   for line in serial.splitlines()) != 1:
                raise RuntimeError(f"expected one {outcome} record for note {number}")
        if status != 33 or "the store holds 1 file(s), 12 bytes" not in serial:
            raise RuntimeError(f"unexpected final store or guest exit {status}")
    finally:
        vm.stop()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path)
    parser.add_argument("guest", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    run(args.kernel.resolve(), args.guest.resolve(), args.output.resolve())
