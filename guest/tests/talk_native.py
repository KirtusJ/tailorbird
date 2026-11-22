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
 T10 while the worker says it is checking a model (2 of 5 MiB, made-up numbers), the screen says
    "Getting ready" and "Checked 2 of 5 MiB", and is not the screen of a model that is ready.
 T9 what is on the screen. The console lines above say how a reply ended and how many bytes it
    kept, not what was drawn (Director 2: `hello` and `hxllo` are the same length). So each capture
    of Tailorbird's screen (answered, stopped, service-stopped, and no-model in the control) is
    compared, every pixel of the whole frame, with the same renderer run on this computer from the
    steps this test took. And for each capture, pictures that must NOT match it are drawn and
    compared too: the reply with one character changed and the length kept, the reply absent, the
    person's words changed, the model's state changed, and the capture before it (a stale screen).
    The capture while PCP OS asks about a note is PCP OS's own screen, and is compared line for
    line as tests/note_roundtrip.py does.

Usage: talk_native.py <kernel.elf32 built with proposals,companion> <tailorbird> <talk-test-worker> <new folder>
This is an emulator, keys sent by the emulator, and a test program in the place of a model.
"""
import argparse
import hashlib
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import time

from note_roundtrip import ASK, exact, panel
from qemu_smoke import Qemu

OPENING = "This is the test worker, not a model. You said: "


def pixels(path):
    data = path.read_bytes()
    head = data.split(b"\n", 3)
    return head[1], data[len(data) - 800 * 600 * 3:]


def frames_tool():
    """The renderer the program uses, built for this computer (guest/examples/talk_frames.rs)."""
    guest = Path(__file__).resolve().parents[1]
    subprocess.run(["cargo", "build", "--quiet", "--example", "talk_frames"], cwd=guest, check=True)
    return guest.parent / "target/debug/examples/talk_frames"


class Screens:
    """Compares captures of Tailorbird's screen with what the renderer draws for the same steps."""

    def __init__(self, tool, output, problems):
        self.tool, self.output, self.problems, self.count, self.before = tool, output, problems, 0, None

    def draw(self, steps, name):
        folder = self.output / ("drawn-%02d-%s" % (self.count, name))
        self.count += 1
        folder.mkdir()
        (folder / "steps.txt").write_text("\n".join(steps + ["shot " + name]) + "\n")
        subprocess.run([str(self.tool), str(folder / "steps.txt"), str(folder), "800", "600"], check=True)
        return pixels(folder / (name + ".ppm"))

    def same(self, capture, steps, others):
        """`capture` must be exactly what `steps` draw, and must differ from each of `others`."""
        seen = pixels(self.output / (capture + ".ppm"))
        if seen != self.draw(steps, capture):
            self.problems.append("T9 %s: the screen is not what the renderer draws for the steps taken" % capture)
        if self.before is not None:
            others = dict(others, **{"the capture before it": None})
        for what, changed in others.items():
            drawn = self.before if changed is None else self.draw(changed, capture + "-not")
            if seen == drawn:
                self.problems.append("T9 %s: the screen also matches a picture it must not (%s)" % (capture, what))
        self.before = seen


def run(kernel, guest, worker, output, with_worker, tool):
    output.mkdir(parents=True, exist_ok=False)
    os_root = Path(__file__).resolve().parents[2] / "pcp-os"
    measure = os_root / "PCP_OS_Rust/crates/pcpos-kernel/tools/measure.py"
    image = subprocess.check_output([sys.executable, measure, kernel], text=True).strip()
    screen = runpy.run_path(str(os_root / "examples/tester-cd/check.py"))
    glyphs = screen["kernel_glyphs"]()
    shutil.copyfile(kernel, output / "kernel.elf32")
    shutil.copyfile(guest, output / "tailorbird.elf")
    shutil.copyfile(worker, output / "talk-test-worker")
    front, back = (hashlib.sha256(p.read_bytes()).hexdigest() for p in (guest, worker))
    command = f"image={image} auto=tailorbird.elf then=off desktop=800x600 propose_image={front}"
    if with_worker:
        command += f" worker_for={front} worker_image={back}"
    vm = Qemu(Path("kernel.elf32"), "tailorbird.elf,talk-test-worker", output, "q35", command)
    problems = []
    screens = Screens(tool, output, problems)
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
            screens.same("no-model", ["notes", "folder taylor", "type hello", "enter"], {
                "the person's words changed": ["notes", "folder taylor", "type hxllo", "enter"],
                "a model that is ready": ["notes", "folder taylor", "ready", "type hello", "enter"],
                "nothing sent": ["notes", "folder taylor", "type hello"]})
            if replies() or any("test worker" in line for line in lines()) or any("tailorbird: model service" in line and "no model service channel" not in line for line in lines()):
                problems.append("T8: without the worker a reply began, a service was reported, or the worker ran")
            vm.key("esc")
            status = vm.proc.wait(timeout=30)
            if status != 33:
                problems.append("T8: status %r, not 33" % status)
            return problems

        # T1
        wait_for("tailorbird: a model service channel is open")
        # T10: while the worker says it is checking, the screen says how far and does not say ready.
        if wait_for("test worker: says it has checked 2 of 5 MiB"):
            time.sleep(0.8)
            vm.screenshot("checking")
            screens.same("checking", ["notes", "folder taylor", "loading 2097152 5242880"], {
                "ready": ["notes", "folder taylor", "ready"],
                "loading with nothing said about how far": ["notes", "folder taylor", "loading"],
                "another amount checked": ["notes", "folder taylor", "loading 3145728 5242880"],
                "all of it checked": ["notes", "folder taylor", "loading 5242880 5242880"],
                "no model": ["notes", "folder taylor"]})
        wait_for("tailorbird: model service: ready")
        said = lines()
        order = [i for i, line in enumerate(said) if "test worker: said it is ready" in line or "tailorbird: model service: ready" in line]
        if len(order) != 2 or "test worker" not in said[order[0]]:
            problems.append("T1: ready was not reported once, after the worker said it")
        # T2
        type_and_send("hello")
        first = ["notes", "folder taylor", "ready", "type hello", "enter", "took", "chunk " + OPENING + "hello", "done"]
        if wait_for("tailorbird: reply 1 kept %d bytes and is complete" % len(OPENING + "hello")):
            vm.screenshot("answered")
            screens.same("answered", first, {
                "one character of the reply changed, the length kept": first[:5] + ["chunk " + OPENING + "hxllo", "done"],
                "one character of the worker's own words changed": first[:5] + ["chunk " + OPENING.replace("not a model", "now a model") + "hello", "done"],
                "no reply yet": first[:5],
                "the person's words changed": ["notes", "folder taylor", "ready", "type hellp", "enter", "took", "chunk " + OPENING + "hello", "done"],
                "the reply failed": first[:6] + ["failed"],
                "the model stopped": first + ["gone"]})
        # T3
        for char in "garden":
            vm.key(char)
        vm.key("alt", "n")
        time.sleep(2)
        vm.screenshot("asked")
        # PCP OS's own screen, not Tailorbird's: exactly the question for this one note.
        rows = screen["read_screen"]((output / "asked.ppm").read_bytes(), glyphs)
        if not exact(rows, panel("taylor/my-first-project", "note-1.txt", "garden", ASK)):
            problems.append("T9 asked: PCP OS's question is not exactly the one for this note: %r" % [row.strip() for row in rows or [] if row.strip()])
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
        kept = int(stopped[0].split()[4]) if len(stopped) == 1 else 0
        second = OPENING + "tell me more about that please"
        # The note that was refused left its words in the field and the Work panel on its result;
        # the next message brings the conversation back.
        until_stop = first + ["type garden", "hide", "clear", "type tell me more about that please", "enter", "took"]
        halted = until_stop + ["chunk " + second[:kept], "stop"]
        if kept:
            screens.same("stopped", halted, {
                "one character of what arrived changed": until_stop + ["chunk " + second[:kept - 1] + "X", "stop"],
                "one character less arrived": until_stop + ["chunk " + second[:kept - 1], "stop"],
                "not stopped: still answering": until_stop + ["chunk " + second[:kept]],
                "complete, not stopped": until_stop + ["chunk " + second[:kept], "done"]})
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
        if kept:
            ended = halted + ["took", "type fail", "enter", "took", "failed", "type leave", "enter", "took", "chunk " + (OPENING + "leave")[:16], "gone", "type anyone", "enter"]
            screens.same("service-stopped", ended, {
                "the model still ready": [step for step in ended if step != "gone"],
                "the last message changed": ended[:-2] + ["type anyonf", "enter"],
                "the failed reply complete": [("done" if step == "failed" else step) for step in ended],
                "one character of what arrived before it stopped changed": [("chunk This is the tesT" if step == "chunk " + (OPENING + "leave")[:16] else step) for step in ended]})
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
    tool = frames_tool()
    found = run(args.kernel.resolve(), args.guest.resolve(), args.worker.resolve(), out / "with-worker", True, tool)
    found += ["control: " + p for p in run(args.kernel.resolve(), args.guest.resolve(), args.worker.resolve(), out / "without-worker", False, tool)]
    if found:
        sys.exit("\n".join(found))
    print("talk: a message is answered, stopped, failed and survives the service ending; without a worker nothing answers; every capture is what the renderer draws and none of the pictures it must not be")
