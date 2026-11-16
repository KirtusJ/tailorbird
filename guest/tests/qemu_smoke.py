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
import select
import subprocess
import tempfile
import time

DEFAULT_TIMEOUT = 100.0


class Qemu:
    def __init__(self, kernel: pathlib.Path, guest: pathlib.Path, folder: pathlib.Path, machine: str,
                 boot_line: str = "auto=tailorbird then=off", display_args=None):
        self.folder = folder
        self.serial_path = folder / "serial.log"
        cmd = [
            "qemu-system-x86_64", "-machine", machine, "-cpu", "max", "-m", "256M",
            "-kernel", str(kernel), "-initrd", str(guest),
            "-append", boot_line,
            "-display", "none", *(display_args if display_args is not None else ["-vga", "std"]),
            "-nic", "none",
            "-serial", "file:" + str(self.serial_path),
            "-no-reboot", "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
            "-qmp", "stdio",
        ]
        print("QEMU_ARGS", json.dumps(cmd), flush=True)
        self.proc = subprocess.Popen(
            cmd, cwd=folder, stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0
        )
        self._response()
        self.qmp("qmp_capabilities")

    def _response(self):
        for _ in range(30):
            if not select.select([self.proc.stdout], [], [], 30)[0]:
                raise TimeoutError("QMP response timed out")
            payload = self.proc.stdout.readline()
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
        self.proc.stdin.write((json.dumps({"execute": method, "arguments": arguments}) + "\n").encode())
        self.proc.stdin.flush()
        return self._response()

    def key(self, *qcodes):
        self.qmp("send-key",
                 keys=[{"type": "qcode", "data": key} for key in qcodes],
                 **{"hold-time": 100})
        time.sleep(0.40)

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
        self.proc.stdin.close()
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


def run(kernel, guest, timeout, machine):
    assert kernel.is_file() and kernel.stat().st_size > 10000
    assert guest.is_file() and guest.stat().st_size > 10000
    with tempfile.TemporaryDirectory(prefix="tailorbird-qmp-") as scratch:
        folder = pathlib.Path(scratch)
        m = Qemu(kernel, guest, folder, machine)
        try:
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if m.proc.poll() is not None:
                    raise RuntimeError("QEMU exited before guest display: " + m.serial()[-1800:])
                try:
                    dims, baseline, first_hash = m.screenshot("initial")
                    # The public kernel selects 1024x768 at default VGA memory.
                    # Wait for actual Tailorbird pixels: the launcher uses this mode too.
                    sample = ((114 + 110) * dims[0] + 32 + 22) * 3
                    if (dims == (1024, 768) and "Tailorbird first frame presented" in m.serial()
                            and baseline[sample:sample + 3] == bytes.fromhex("18364f")):
                        break
                except RuntimeError as e:
                    # QMP screen may be transient during mode negotiation.
                    print("SCREEN_PENDING", str(e)[:160], flush=True)
                time.sleep(0.3)
            else:
                raise TimeoutError("No Tailorbird frame at the inherited 1024x768 scanout. last_screen=" + str(dims)
                                   + " serial=" + m.serial()[-1800:])

            left, top = (dims[0] - 960) // 2, (dims[1] - 540) // 2
            editor = (left + 140, top + 460, left + 920, top + 500)
            base = crop(baseline, dims[0], editor)
            m.key("a")
            # Wait for the new frame independently of host scheduling and key delivery.
            deadline = time.monotonic() + timeout
            while True:
                dims_a, pixels_a, hash_a = m.screenshot("lower")
                if crop(pixels_a, dims[0], editor) != base or time.monotonic() >= deadline:
                    break
                time.sleep(0.08)
            assert dims_a == dims
            lower = crop(pixels_a, dims[0], editor)
            assert lower != base, "Typing lowercase a did not change request pixels"

            # The native keyboard parser must not mistake unsupported Ctrl
            # shortcuts for literal editor bytes, deletion or Submit. Use
            # real QMP-modifier events on both emulated machine types and
            # compare the complete framebuffer, not merely a host reducer.
            # The guest is offline, so no backend effect is exercised here.
            for label, chord in [
                ("ctrl_letter", ("ctrl", "b")),
                ("ctrl_backspace", ("ctrl", "backspace")),
                ("ctrl_enter", ("ctrl", "ret")),
            ]:
                m.key(*chord)
                dims_ctrl, ctrl_pixels, ctrl_hash = m.screenshot(label)
                assert dims_ctrl == dims
                assert ctrl_pixels == pixels_a, (
                    f"Unsupported {chord} changed the native screen, "
                    "prompt or submission state"
                )
                print("CTRL_CHORD_INERT", label, ctrl_hash, flush=True)

            # Deliberately invisible trailing space: the text glyphs alone
            # cannot distinguish "a" from "a ", but the displayed exact
            # byte count must. It sits to the left of the editor raster.
            counter = (left + 20, top + 485, left + 135, top + 503)
            count_a = crop(pixels_a, dims[0], counter)
            m.key("spc")
            dims_space, pixels_space, hash_space = m.screenshot("trailing_space")
            assert dims_space == dims
            assert crop(pixels_space, dims[0], editor) == lower, (
                "A space glyph should preserve the visible text cells"
            )
            assert crop(pixels_space, dims[0], counter) != count_a, (
                "Trailing space increased submitted bytes but no length cue"
            )
            m.key("backspace")
            dims_no_space, pixels_no_space, _ = m.screenshot("space_removed")
            assert dims_no_space == dims
            assert crop(pixels_no_space, dims[0], counter) == count_a
            assert crop(pixels_no_space, dims[0], editor) == lower
            print("PASS: QEMU trailing-space byte-count changes visibly", flush=True)

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

            # Positive/negative discriminator for PS/2 queue saturation:
            # two smaller QMP batches with a measured drain interval, but
            # otherwise the identical key events and the same guest/parser.
            # This does NOT replace the mandatory one-command adverse arm.
            prefix = tap("z") + tap("x") + tap("z")
            suffix = ([ev("shift", True)] + tap("a")
                      + [ev("shift", False)] + tap("a"))
            m.qmp("input-send-event", events=prefix)
            time.sleep(0.40)
            m.qmp("input-send-event", events=suffix)
            time.sleep(0.40)
            dims_split, split_pixels, hash_split = m.screenshot("pt_split_batches")
            assert dims_split == dims
            split_editor = crop(split_pixels, dims[0], editor)
            print("PT_SPLIT_BATCH_CONTROL", json.dumps({
                "split_matches_spaced": split_editor == ordered_editor,
                "sha256": hash_split,
                "spaced_sha256": hash_ordered,
                "prefix_event_count": len(prefix),
                "suffix_event_count": len(suffix),
            }), flush=True)
            # Clear the actual displayed editor even if the split-batch
            # control failed. The baseline was empty before this arm.
            for _ in range(6):
                m.key("backspace")

            batch = prefix + suffix
            m.qmp("input-send-event", events=batch)
            time.sleep(0.30)
            dims_burst, burst_pixels, hash_burst = m.screenshot("pt_ordered_burst")
            assert dims_burst == dims
            burst_editor = crop(burst_pixels, dims[0], editor)
            # Read the exact source-pinned kernel glyphs rather than using
            # a QEMU "spaced" screenshot as the expected answer. That earlier
            # oracle was invalid: its fifth cell was actually blank.
            # Every colour/pixel below comes from the same table and blend
            # algorithm used by Tailorbird's exact_ascii.rs renderer.
            font_path = (pathlib.Path(__file__).resolve().parents[4]
                         / "PCP_OS_Rust/crates/pcpos-kernel/src/font.rs")
            font_source = font_path.read_text()
            glyph_rows = re.findall(r"^\s*\[([0-9,\s]+)\],?\s*$",
                                    font_source, flags=re.MULTILINE)
            glyphs = []
            for raw in glyph_rows:
                values = [int(v.strip()) for v in raw.split(",") if v.strip()]
                if len(values) == 200:
                    glyphs.append(values)
            assert len(glyphs) == 95, "font source did not yield all 95 glyphs"

            def source_glyph(ch):
                glyph = glyphs[ord(ch) - 32]
                bg, fg = ((0x18, 0x36, 0x4F), (0xF2, 0xF7, 0xFC))
                result = bytearray()
                for coverage in glyph:
                    for under, over in zip(bg, fg):
                        result.append((under * (255 - coverage) +
                                       over * coverage + 127) // 255)
                return bytes(result)

            def cell(editor_pixels, index):
                cx, cy = (index % 78) * 10, (index // 78) * 20
                return b"".join(
                    editor_pixels[((cy + dy) * 780 + cx) * 3:
                                  ((cy + dy) * 780 + cx + 10) * 3]
                    for dy in range(20)
                )

            expected = bytearray(base)
            intended = "zxzAa"
            for i, ch in enumerate(intended):
                raster = source_glyph(ch)
                cx, cy = (i % 78) * 10, (i // 78) * 20
                for y in range(20):
                    dest = ((cy + y) * 780 + cx) * 3
                    expected[dest:dest + 30] = raster[y * 30:(y + 1) * 30]
            expected = bytes(expected)

            all_glyphs = {chr(n): source_glyph(chr(n))
                          for n in range(32, 127)}

            def diagnose_editor(editor_pixels):
                diagnosed = []
                for i in range(8):
                    value = cell(editor_pixels, i)
                    distances = {
                        name: sum(value[j:j+3] != template[j:j+3]
                                  for j in range(0, len(value), 3))
                        for name, template in all_glyphs.items()
                    }
                    exact = [ch for ch, distance in distances.items()
                             if distance == 0]
                    closest = sorted(distances.items(),
                                     key=lambda pair: pair[1])[:5]
                    diagnosed.append({
                        "cell": i, "exact": exact,
                        "closest_five": closest,
                        "is_blank": value == all_glyphs[" "],
                    })
                return diagnosed

            verdicts = {
                "paced_equals_source": ordered_editor == expected,
                "split_equals_source": split_editor == expected,
                "burst_equals_source": burst_editor == expected,
                "burst_equals_paced": burst_editor == ordered_editor,
                "paced_fifth_is_blank": cell(ordered_editor, 4)
                    == all_glyphs[" "],
                "lower_a_matches_source": cell(lower, 0)
                    == all_glyphs["a"],
                "upper_A_matches_source": cell(upper, 0)
                    == all_glyphs["A"],
            }
            print("PT_SOURCE_GLYPH_DIAGNOSIS", json.dumps({
                "verdicts": verdicts,
                "paced_first_eight": diagnose_editor(ordered_editor),
                "split_first_eight": diagnose_editor(split_editor),
                "burst_first_eight": diagnose_editor(burst_editor),
                "spaced_sha256": hash_ordered,
                "split_sha256": hash_split,
                "full_batch_sha256": hash_burst,
                "previous_bad_sha256": hash_collapsed,
                "serial_excerpt": m.serial()[-1200:],
            }), flush=True)
            assert verdicts["lower_a_matches_source"]
            assert verdicts["upper_A_matches_source"]
            assert burst_editor != old_bad_editor, (
                "PT counterexample: batched input collapsed to azx pixels"
            )
            assert verdicts["burst_equals_source"], (
                "PT counterexample: full-batch pixels differ from exact zxzAa"
            )
            print("PASS: full-batch QMP exactly matches source-font zxzAa",
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
                "trailing_space": hash_space,
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
    parser.add_argument("--machine", choices=("pc", "q35"), default="pc")
    args = parser.parse_args()
    run(args.kernel.resolve(), args.guest.resolve(), args.timeout, args.machine)
