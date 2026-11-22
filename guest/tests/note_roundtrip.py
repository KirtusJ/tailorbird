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

ASK = "Page 1 of 1. To allow: press F12, then Enter. Esc refuses. Arrows turn pages."
CONFIRM = "Now press Enter to allow it. Any other key refuses."


def panel(folder, name, document, last):
    """Every line the kernel's question must show for this one file, in order, and nothing else."""
    return ["PCP OS ASKS YOU",
            "Taylor wants to make a new file. Nothing has been made yet.",
            f"Folder: {folder} File: {name}",
            f"Size: {len(document) + 1} characters. It will last until the computer is switched off.",
            document,
            "(end of the file)",
            last]


def exact(rows, expected):
    """Whether the rows read off the display are exactly the expected lines.

    The display wraps a long line at a word, so the last expected line may arrive as two rows; no
    other difference is allowed: not a longer name, not a line more, not another page count.
    A substring test here accepted a changed body, file name, folder and page count (Director 2).
    """
    seen = [row.strip() for row in rows or [] if row.strip()]
    if seen[:len(expected) - 1] != expected[:-1]:
        return False
    return " ".join(seen[len(expected) - 1:]) == expected[-1] and len(seen) - len(expected) in (0, 1)


def selftest_exact():
    folder, name, document = "taylor/my-first-project", "note-1.txt", "garden note"
    good = panel(folder, name, document, ASK)
    shown = ["  " + line for line in good[:-1]] + ["  Page 1 of 1. To allow: press F12, then Enter. Esc refuses. Arrows turn", "  pages."]
    assert exact(shown, good) and exact(["  " + line for line in good], good)
    changed = {
        "a longer body": panel(folder, name, "garden notebook", ASK),
        "a file name with a suffix": panel(folder, name + ".bak", document, ASK),
        "another folder": panel(folder + "-other", name, document, ASK),
        "another page count": panel(folder, name, document, ASK.replace("Page 1 of 1.", "Page 1 of 10.")),
        "the panel of another note": panel(folder, "note-2.txt", document, ASK),
        "the second screen in place of the first": panel(folder, name, document, CONFIRM),
    }
    for what, lines in changed.items():
        assert not exact(["  " + line for line in lines], good), what
    extra = good[:5] + ["secret next line"] + good[5:]
    assert not exact(["  " + line for line in extra], good), "a line more before the end"
    assert not exact(["  " + line for line in good[:2] + good[3:]], good), "no folder and file line"
    assert not exact(["  " + line for line in good] + ["  more"], good), "a line after the last"
    assert not exact([], good) and not exact(None, good)


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

        def shows(tag, expected):
            def matches():
                vm.screenshot(tag)
                return exact(screen["read_screen"]((output / (tag + ".ppm")).read_bytes(), glyphs), expected)
            wait_for(matches, tag + ": the question does not show exactly the file asked about")

        def preview(number):
            vm.key("alt", "n")
            shows(f"preview-{number}", panel(folder, f"note-{number}.txt", document, ASK))

        wait_for(lambda: "Tailorbird first frame presented" in vm.serial(), "first frame")
        for char in document:
            vm.key("spc" if char == " " else char)
        preview(1)
        vm.key("esc")
        wait_for(lambda: "tailorbird: Not made: you said no." in vm.serial(), "first refusal")
        preview(2)
        vm.key("f12")
        # The second screen is still about the same file, whole, before Enter is pressed.
        shows("confirm", panel(folder, "note-2.txt", document, CONFIRM))
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
    selftest_exact()
    run(args.kernel.resolve(), args.guest.resolve(), args.output.resolve())
