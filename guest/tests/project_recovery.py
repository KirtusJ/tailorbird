"""Cold-boot Tailorbird after two power cuts, normal shutdown and a kernel Stop."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import struct
import subprocess
import sys
import time

from note_roundtrip import ASK, CONFIRM, exact, panel
from qemu_smoke import Qemu

ROOT = Path(__file__).resolve().parents[2]
FOLDER = "taylor/my-first-project"
NOTE = "recovery note"
SAVED = b"existing project\n"


def seeded_volume(out):
    volume = out / "volume"
    identity = runpy.run_path(str(ROOT / "pcp-os/examples/vm-store/create.py"))["create"](volume)
    disk = volume / "data.raw"
    image = bytearray(disk.read_bytes())
    name = (FOLDER + "/kept.txt").encode()
    body = struct.pack("<HI", len(name), len(SAVED)) + name + SAVED
    header = b"PCPSLOT1" + struct.pack("<QI", 2, len(body)) + hashlib.sha256(body).digest()
    image[130*512:130*512+84] = header + hashlib.sha256(header).digest()
    image[131*512:131*512+len(body)] = body
    disk.write_bytes(image)
    return disk, identity, bytes(image)


def run(kernel, guest, out):
    out.mkdir(parents=True, exist_ok=False)
    disk, identity, initial = seeded_volume(out)
    fixture = runpy.run_path(str(ROOT / "pcp-os/examples/optical-disk-check/check.py"))
    nvme = out / "unselected-nvme.raw"
    baseline, marker = fixture["make_gpt_disk"](4096)
    nvme.write_bytes(baseline)
    fixture["check_gpt_fixture"](nvme, 4096)
    assert baseline[marker:marker+len(fixture["MARKER"])] == fixture["MARKER"]
    nvme_digest = hashlib.sha256(baseline).hexdigest()
    image = subprocess.check_output(
        [sys.executable, ROOT / "pcp-os/PCP_OS_Rust/crates/pcpos-kernel/tools/measure.py", kernel],
        text=True).strip()
    digest = hashlib.sha256(guest.read_bytes()).hexdigest()
    reader = runpy.run_path(str(ROOT / "pcp-os/examples/tester-cd/check.py"))
    glyphs = reader["kernel_glyphs"]()
    reports = []
    for action in ("cut-at-confirmation", "clean-shutdown", "cut-after-made", "missing-store",
                   "wrong-volume-id", "kernel-stop"):
        folder = out / action
        folder.mkdir()
        shutil.copyfile(kernel, folder / "k")
        shutil.copyfile(guest, folder / "t.elf")
        requested_id = identity if action != "wrong-volume-id" else f"{int(identity, 16) ^ 1:032x}"
        cmd = f"image={image} auto=t.elf then=menu desktop=800x600 propose_image={digest} store=ide0 store_id={requested_id}"
        devices = ["-vga", "std", "-drive", f"if=none,id=sentinel,format=raw,snapshot=off,file={nvme}",
                   "-device", "nvme,drive=sentinel,serial=PCPTEST,logical_block_size=4096,physical_block_size=4096"]
        if action != "missing-store":
            devices += ["-drive", f"if=none,id=store,format=raw,file={disk}",
                        "-device", "ide-hd,drive=store,bus=ide.0,unit=0,serial=PCP-VM-STORE"]
        vm = Qemu(Path("k"), Path("t.elf"), folder, "pc", cmd, display_args=devices)

        def wait(check, description):
            deadline = time.monotonic() + 40
            while vm.proc.poll() is None and time.monotonic() < deadline:
                if check():
                    return
                time.sleep(0.12)
            raise RuntimeError(f"{action}: timed out waiting for {description}: {vm.serial()[-900:]}")

        def screen(tag, check):
            def matches():
                vm.screenshot(tag)
                rows = reader["read_screen"]((folder / (tag + ".ppm")).read_bytes(), glyphs)
                return rows is not None and check(rows)
            wait(matches, tag)

        def power_question(tag):
            vm.key("f10")
            screen(tag, lambda rows: any("Switch PCP OS off?" in row for row in rows))

        try:
            refused = action in ("missing-store", "wrong-volume-id")
            if refused:
                vm.proc.wait(timeout=45)
                assert "requested VM checkpoint store could not be mounted; no program started" in vm.serial()
                assert "Tailorbird first frame presented" not in vm.serial()
            else:
                wait(lambda: "Tailorbird first frame presented" in vm.serial(), "first frame")
                assert "VM checkpoint store mounted: 1 file(s)" in vm.serial()
            if action == "kernel-stop":
                vm.key("alt", "p")
                vm.key("ctrl", "alt", "delete")
                wait(lambda: "t.elf ended STOPPED" in vm.serial(), "kernel Stop from Projects")
                power_question("shutdown-question")
                vm.key("ret")
            elif not refused:
                for ch in NOTE:
                    vm.key("spc" if ch == " " else ch)
                vm.key("alt", "n")
                for tag, last in [("ask", ASK), ("confirm", CONFIRM)]:
                    screen(tag, lambda rows: exact(rows, panel(FOLDER, "note-1.txt", NOTE, last)))
                    if tag == "ask":
                        vm.key("f12")
                if action != "cut-at-confirmation":
                    vm.key("ret")
                    wait(lambda: f"Made {FOLDER}/note-1.txt. Kept until power-off." in vm.serial(), "receipt")
                if action == "clean-shutdown":
                    vm.key("esc")
                    wait(lambda: "t.elf ended DONE" in vm.serial(), "app exit")
                    power_question("shutdown-cancel")
                    vm.key("esc")
                    vm.key("spc")
                    vm.key("ret")
                    wait(lambda: vm.serial().count("Tailorbird first frame presented") == 2, "relaunch after cancel")
                    vm.key("esc")
                    wait(lambda: vm.serial().count("t.elf ended DONE") == 2, "second app exit")
                    power_question("shutdown-confirm")
                    vm.key("ret")
            if action.startswith("cut-"):
                assert vm.proc.poll() is None
                vm.proc.kill()
                assert vm.proc.wait(timeout=10) < 0
            elif not refused:
                assert vm.proc.wait(timeout=30) == 33
                assert "ALL SELF-TESTS PASS" in vm.serial()
            made = "CREATED after the person allowed it" in vm.serial()
            assert made == (action in ("clean-shutdown", "cut-after-made"))
        finally:
            vm.stop()
        assert disk.read_bytes() == initial, f"{action}: existing volume bytes changed"
        assert nvme.read_bytes() == baseline, f"{action}: unselected NVMe disk changed"
        reports.append({"action": action, "exit": vm.proc.returncode, "mounted_files": None if refused else 1,
                        "confirmed_creation": made, "store_sha256": hashlib.sha256(initial).hexdigest(),
                        "unselected_4k_nvme_sha256": nvme_digest})
        (out / "result.json").write_text(json.dumps({"kernel": image, "guest": digest, "boots": reports}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for argument in ("kernel", "guest", "output"):
        parser.add_argument(argument, type=Path)
    args = parser.parse_args()
    run(args.kernel.resolve(), args.guest.resolve(), args.output.resolve())
