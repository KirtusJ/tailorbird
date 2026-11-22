"""Talk with a stand-in for a model service, on PCP OS, from the keyboard to the screen and back.

Tailorbird runs with a second program beside it on PCP OS's channel. That program is the TEST
WORKER (guest/src/talk_test_worker_main.rs): NOT A MODEL. It says so and repeats what it is sent.
What is checked is the channel, the records, Stop, and what Tailorbird believes about the service.

Predicates, fixed before the first run.
 T1 with the worker named on the loader's line, Tailorbird reports a channel, then the service
    ready, only after the worker said so.
 T2 a typed message is answered: the reply is complete and keeps exactly the bytes the worker sent
    (48 bytes of its own words and the message).
 T3 Alt+N while the worker is there still asks the person, and Esc refuses: nothing is made.
 T4 Alt+S during an answer stops it: the reply is marked stopped with fewer bytes than the whole
    answer, and the worker says it was told and stopped.
 T5 a reply the worker fails is marked failed with 0 bytes.
 T6 when the worker ends in the middle of an answer the service is reported stopped (not "did not
    start": it had been ready), that reply is failed with what had arrived, and a later message
    starts no reply.
 T7 Esc ends Tailorbird, PCP OS ends the worker's run, and the emulator ends with status 33.
 T8 (control) the same Tailorbird without the worker's words on the line: no channel, a typed
    message starts no reply, and the worker is never started.

Usage: talk_native.py <kernel.elf32 built with proposals,companion> <tailorbird> <talk-test-worker> <new folder>
This is an emulator, keys sent by the emulator, and a test program in the place of a model.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import time

from qemu_smoke import Qemu

OPENING = "This is the test worker, not a model. You said: "


def run(kernel, guest, worker, output, with_worker):
    output.mkdir(parents=True, exist_ok=False)
    os_root = Path(__file__).resolve().parents[2] / "pcp-os"
    measure = os_root / "PCP_OS_Rust/crates/pcpos-kernel/tools/measure.py"
    image = subprocess.check_output([sys.executable, measure, kernel], text=True).strip()
    shutil.copyfile(kernel, output / "kernel.elf32")
    shutil.copyfile(guest, output / "tailorbird.elf")
    shutil.copyfile(worker, output / "talk-test-worker")
    front, back = (hashlib.sha256(p.read_bytes()).hexdigest() for p in (guest, worker))
    command = f"image={image} auto=tailorbird.elf then=off desktop=800x600 propose_image={front}"
    if with_worker:
        command += f" worker_for={front} worker_image={back}"
    vm = Qemu(Path("kernel.elf32"), "tailorbird.elf,talk-test-worker", output, "q35", command)
    problems = []
    try:
        def lines():
            return vm.serial().splitlines()

        def wait_for(words, seconds=40):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline and vm.proc.poll() is None:
                if any(words in line for line in lines()):
                    return True
                time.sleep(0.1)
            problems.append("never said: %r" % words)
            return False

        def type_and_send(words):
            for char in words:
                vm.key("spc" if char == " " else char)
            vm.key("ret")

        def replies():
            return [line for line in lines() if line.startswith("tailorbird: reply ")]

        wait_for("Tailorbird first frame presented")
        if not with_worker:
            if not wait_for("tailorbird: no model service channel"):
                return problems
            type_and_send("hello")
            time.sleep(2)
            vm.screenshot("no-model")
            if replies() or any("test worker" in line for line in lines()) or any("tailorbird: model service" in line and "no model service channel" not in line for line in lines()):
                problems.append("T8: without the worker a reply began, a service was reported, or the worker ran")
            vm.key("esc")
            status = vm.proc.wait(timeout=30)
            if status != 33:
                problems.append("T8: status %r, not 33" % status)
            return problems

        # T1
        wait_for("tailorbird: a model service channel is open")
        wait_for("tailorbird: model service: ready")
        said = lines()
        order = [i for i, line in enumerate(said) if "test worker: said it is ready" in line or "tailorbird: model service: ready" in line]
        if len(order) != 2 or "test worker" not in said[order[0]]:
            problems.append("T1: ready was not reported once, after the worker said it")
        # T2
        type_and_send("hello")
        if wait_for("tailorbird: reply 1 kept %d bytes and is complete" % len(OPENING + "hello")):
            vm.screenshot("answered")
        # T3
        for char in "garden":
            vm.key(char)
        vm.key("alt", "n")
        time.sleep(2)
        vm.screenshot("asked")
        vm.key("esc")
        wait_for("tailorbird: Not made: you said no.")
        # The words of the note are still in the field: take them out before the next message.
        for _ in "garden":
            vm.key("backspace")
        # T4
        type_and_send("tell me more about that please")
        wait_for("tailorbird: reply 2 is being answered")
        vm.key("alt", "s")
        wait_for("test worker: told to stop, and stopped")
        whole = len(OPENING + "tell me more about that please")
        stopped = [line for line in replies() if line.startswith("tailorbird: reply 2 kept ") and line.endswith("stopped by the person")]
        deadline = time.monotonic() + 20
        while not stopped and time.monotonic() < deadline:
            time.sleep(0.2)
            stopped = [line for line in replies() if line.startswith("tailorbird: reply 2 kept ") and line.endswith("stopped by the person")]
        if len(stopped) != 1 or not 0 < int(stopped[0].split()[4]) < whole:
            problems.append("T4: reply 2 was not stopped part of the way: %r" % stopped)
        vm.screenshot("stopped")
        time.sleep(1.5)
        # T5
        type_and_send("fail")
        wait_for("tailorbird: reply 3 kept 0 bytes and is failed")
        # T6
        type_and_send("leave")
        wait_for("tailorbird: model service: stopped")
        wait_for("tailorbird: reply 4 kept 16 bytes and is failed")
        type_and_send("anyone")
        time.sleep(2)
        vm.screenshot("service-stopped")
        if any(line.startswith("tailorbird: reply 5") for line in replies()):
            problems.append("T6: a reply began after the service had stopped")
        # T7
        vm.key("esc")
        status = vm.proc.wait(timeout=30)
        serial = vm.serial()
        if status != 33:
            problems.append("T7: status %r, not 33" % status)
        if "CREATED" in serial or serial.count("DENIED") != 1:
            problems.append("T3: the refused note was made, or was not refused exactly once")
        if "talk-test-worker" not in serial:
            problems.append("T7: PCP OS did not name the worker it started")
    finally:
        vm.stop()
        (output / "replies.txt").write_text("\n".join(line for line in vm.serial().splitlines() if line.startswith(("tailorbird:", "test worker:", "companion"))) + "\n")
    return problems


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("kernel", type=Path)
    parser.add_argument("guest", type=Path)
    parser.add_argument("worker", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    found = run(args.kernel.resolve(), args.guest.resolve(), args.worker.resolve(), out / "with-worker", True)
    found += ["control: " + p for p in run(args.kernel.resolve(), args.guest.resolve(), args.worker.resolve(), out / "without-worker", False)]
    if found:
        sys.exit("\n".join(found))
    print("talk: a message is answered, stopped, failed and survives the service ending; without a worker nothing answers")
