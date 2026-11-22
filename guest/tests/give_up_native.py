"""Taylor gives up making files when the person says so, and PCP OS holds her to it.

The person presses Alt+G twice. Tailorbird then tells PCP OS that it gives up proposing files, and
says so on the screen only on PCP OS's own receipt. After that PCP OS would refuse any proposal from
this run without asking; Tailorbird does not even ask, and says why.

Predicates, fixed before the first run.
 G1 Alt+G once tells PCP OS nothing: its log has no line that the program gave up, and Alt+N still
    brings PCP OS's question, which Esc refuses.
 G2 a key pressed between two Alt+G takes the first back: after Alt+G, a letter, Alt+G, PCP OS has
    still been told nothing.
 G3 Alt+G twice: PCP OS's log says exactly once that the program GAVE UP proposing files, and
    Tailorbird's says "PCP OS confirmed: Taylor makes no more files until started again."
 G4 Alt+N after that asks PCP OS nothing: no further proposal appears in PCP OS's log, Tailorbird
    says the confirmed words again, and nothing was made in the whole run (no proposal is recorded
    as CREATED and Tailorbird never says a file was made).
 G5 Esc ends Tailorbird and the emulator ends with status 33.
 G6 (control, with --older: a kernel with proposals built before the give-up record existed) Alt+G
    twice gets no confirmation: Tailorbird says "PCP OS did not confirm whether Taylor can still make
    files.", never the confirmed words and never that nothing changed; Alt+N then shows what is so,
    which on that kernel is that PCP OS still asks.

The first run of this file failed G4 by this file's own error: it looked for the word CREATED
anywhere, which PCP OS's register also uses of processes, and for a line about the store that PCP OS
does not print when the store is empty. Both runs had behaved as required.

Usage: give_up_native.py <kernel.elf32 with proposals> <tailorbird> <new folder> [--older <kernel.elf32>]
This is an emulator, with keys sent by the emulator.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import time

from qemu_smoke import Qemu

CONFIRMED = "tailorbird: PCP OS confirmed: Taylor makes no more files until started again."
UNCONFIRMED = "tailorbird: PCP OS did not confirm whether Taylor can still make files."
GAVE = "GAVE UP proposing files"


def run(kernel, guest, output, older):
    output.mkdir(parents=True, exist_ok=False)
    os_root = Path(__file__).resolve().parents[2] / "pcp-os"
    measure = os_root / "PCP_OS_Rust/crates/pcpos-kernel/tools/measure.py"
    image = subprocess.check_output([sys.executable, measure, kernel], text=True).strip()
    shutil.copyfile(kernel, output / "kernel.elf32")
    shutil.copyfile(guest, output / "tailorbird.elf")
    command = f"image={image} auto=tailorbird.elf then=off desktop=800x600 propose_image={hashlib.sha256(guest.read_bytes()).hexdigest()}"
    problems = []
    vm = Qemu(Path("kernel.elf32"), Path("tailorbird.elf"), output, "q35", command)
    try:
        def wait_for(words, count=1, seconds=30):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline and vm.proc.poll() is None:
                if vm.serial().count(words) >= count:
                    return True
                time.sleep(0.1)
            return False

        def asked(attempt):
            """Alt+N brings PCP OS's question, and Esc refuses it: one DENIED record for that attempt."""
            vm.key("alt", "n")
            time.sleep(2)
            vm.key("esc")
            return wait_for(f"proposal 1 of attempt {attempt}: ") and "DENIED" in [line for line in vm.serial().splitlines() if line.startswith(f"proposal 1 of attempt {attempt}: ")][0]

        if not wait_for("Tailorbird first frame presented"):
            raise RuntimeError(f"no first frame; see {output}")
        for char in "note":
            vm.key(char)
        vm.key("alt", "g")
        time.sleep(1)
        if GAVE in vm.serial() or not asked(1):
            problems.append("G1: one Alt+G told PCP OS something, or Alt+N no longer asked")
        vm.key("alt", "g")
        vm.key("x")
        vm.key("alt", "g")
        time.sleep(1)
        if GAVE in vm.serial() or CONFIRMED in vm.serial() or UNCONFIRMED in vm.serial():
            problems.append("G2: a key between the two presses did not take the first back")
        vm.key("alt", "g")
        if older:
            if not wait_for(UNCONFIRMED) or CONFIRMED in vm.serial() or GAVE in vm.serial():
                problems.append("G6: without the kernel's confirmation Tailorbird did not say so, or claimed it")
            if not asked(3):
                problems.append("G6: Alt+N no longer brought PCP OS's question")
        else:
            if not wait_for(CONFIRMED) or vm.serial().count(GAVE) != 1:
                problems.append("G3: PCP OS did not say once that the program gave up, or Tailorbird did not say it was confirmed")
            before = vm.serial().count("proposal ")
            vm.key("alt", "n")
            time.sleep(3)
            vm.key("alt", "g")
            time.sleep(1)
            if vm.serial().count("proposal ") != before or vm.serial().count(GAVE) != 1:
                problems.append("G4: after giving up, PCP OS was asked again or told again")
        vm.key("esc")
        status = vm.proc.wait(timeout=30)
        serial = vm.serial()
        if status != 33:
            problems.append(f"G5: status {status}, not 33")
        if any(line.startswith("proposal ") and "CREATED" in line for line in serial.splitlines()) or "tailorbird: Made " in serial:
            problems.append("G4: something was made in this run")
    finally:
        vm.stop()
    return problems


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("kernel", type=Path)
    parser.add_argument("guest", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--older", type=Path)
    args = parser.parse_args()
    args.output.resolve().mkdir(parents=True, exist_ok=False)
    problems = run(args.kernel.resolve(), args.guest.resolve(), args.output.resolve() / "with", False)
    if args.older:
        problems += run(args.older.resolve(), args.guest.resolve(), args.output.resolve() / "older", True)
    else:
        print("G6: NOT RUN (no --older kernel given)")
    if problems:
        sys.exit("\n".join(problems))
    print("give up: Alt+G twice gives up making files on PCP OS's confirmation, once does not, and afterwards nothing is asked")
