#!/usr/bin/env python3
"""Exercise Tailorbird's display preference on a measured public desktop."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import time

from qemu_smoke import Qemu, crop

# Computed directly from the kernel's 20x40 coverage masks and Tailorbird palette.
TITLE = {
    "fit": "a35240f2eea7b8ccc7536d9d8e8402659126f5dc8862590605fbb60544fb0322",
    "compact": "a68f1e7a3d3da0f769d39c88478ab179460ca11b4b0def00437561838a995b2f",
}
# Source-bound to the fixed model-free 800x600 scale kernel and Tailorbird
# renderer. Measured from all four original native title crops in #262 CI.
# Unlike a one-pixel color presence test, this guards the entire title.
SMALL_TITLE = "56fe7c0141736d9bb2a83328588454cc773da555d7038c6935c09b6df7d2420b"
TEAL = bytes.fromhex("41bad7")
PANEL = bytes.fromhex("18364f")
TEXT = bytes.fromhex("f2f7fc")


# Independent source-font oracle for the disabled Stop caption. Checking the
# full original RGB glyph cells catches missing faint letters that a contrast
# threshold or a single-color-presence assertion would silently accept.
FONT = Path(__file__).resolve().parents[4] / "PCP_OS_Rust/crates/pcpos-kernel/src/font.rs"
GROUND = bytes.fromhex("10283f")
SOFT = bytes.fromhex("9fb4c8")


def kernel_glyphs():
    source = FONT.read_text()
    marker = "pub static GLYPHS: [[u8; 200]; 95] = ["
    if source.count(marker) != 1:
        raise AssertionError("kernel font table declaration changed")
    values = [int(x) for x in re.findall(r"\b\d+\b",
              source.split(marker, 1)[1].split("];", 1)[0])]
    if len(values) != 95 * 200 or any(x > 255 for x in values):
        raise AssertionError("kernel font coverage table malformed")
    return [values[i:i+200] for i in range(0, len(values), 200)]


def stop_cell_rgb(glyphs, ch, width, height):
    """Separate Python replay of Frame::glyph's exact 10x20 box filter/blend."""
    if width < 1 or height < 1 or width > 10 or height > 20:
        raise AssertionError("outside original single-size font regime")
    mask = glyphs[ord(ch) - 32]
    result = bytearray()
    for y in range(height):
        y0, y1 = y*20//height, max((y+1)*20//height, y*20//height+1)
        for x in range(width):
            x0, x1 = x*10//width, max((x+1)*10//width, x*10//width+1)
            alpha = sum(mask[yy*10+xx] for yy in range(y0,y1)
                        for xx in range(x0,x1)) // ((x1-x0)*(y1-y0))
            result.extend((base*(255-alpha)+ink*alpha+127)//255
                          for base,ink in zip(GROUND,SOFT))
    return bytes(result)


def require_stop_receipts(receipts, small):
    """Refuse success if any source-RGB frame or erased-letter check was skipped."""
    expected = {"native_frames": 4 if small else 1,
                "erased_letter_negatives": 8 if small else 2}
    if receipts != expected:
        raise AssertionError(f"disabled Stop oracle checks incomplete: {receipts!r} != {expected!r}")


def selftest_stop_receipts():
    """Host-only missing-check controls, independent of actual QEMU frames."""
    for small in (False, True):
        expected = {"native_frames": 4 if small else 1,
                    "erased_letter_negatives": 8 if small else 2}
        require_stop_receipts(dict(expected), small)
        for key in expected:
            missing = dict(expected)
            missing[key] -= 1
            try:
                require_stop_receipts(missing, small)
            except AssertionError:
                pass
            else:
                raise AssertionError(f"missing {key} not refused at small={small}")
    print("Stop receipt gate: 2 valid and 4 skipped-check adverse controls PASS", flush=True)


def check(kernel, app, measurement, folder, small):
    folder.mkdir(parents=True, exist_ok=False)
    w, h = (800, 600) if small else (1920, 1080)
    report = {"size": [w,h], "measurement": measurement, "frames": {},
              "app_sha256": hashlib.sha256(app.read_bytes()).hexdigest(),
              "kernel_sha256": hashlib.sha256(kernel.read_bytes()).hexdigest()}
    m = Qemu(kernel, app, folder, "q35", f"image={measurement} desktop={w}x{h} ui_scale=1",
             ["-accel", "tcg", "-icount", "shift=3", "-vga", "none", "-device", "VGA,vgamem_mb=32"])
    def wait_serial(marker):
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if marker in m.serial():
                return
            if m.proc.poll() is not None:
                break
            time.sleep(.1)
        raise RuntimeError(f"missing {marker!r}: {m.serial()[-900:]}")

    def sample(name, accepts):
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            dims, rgb, digest = m.screenshot(name)
            if dims == (w,h) and accepts(rgb):
                report["frames"][name] = {"size": list(dims), "ppm_sha256": digest}
                return rgb
            time.sleep(.15)
        raise RuntimeError(f"{name}: expected paint missing")

    def menu(rgb):
        return (crop(rgb,w,(21,110,22,111)) == TEAL
                and crop(rgb,w,(25,110,26,111)) == PANEL
                and TEXT in crop(rgb,w,(30,100,130,120)))

    def box(mode,x,y,width,height):
        if small:
            return (x*5//6, 75+y*5//6, (x+width)*5//6, 75+(y+height)*5//6)
        k = 2 if mode == "fit" else 1
        left, top = (w-960*k)//2, (h-540*k)//2
        return (left+x*k,top+y*k,left+(x+width)*k,top+(y+height)*k)

    def area(rgb, mode, x,y,width,height):
        return crop(rgb,w,box(mode,x,y,width,height))

    def form(rgb, mode):
        title = area(rgb,mode,22,16,180,30)
        expected = SMALL_TITLE if small else TITLE[mode]
        if small and area(rgb,mode,3,5,1,1) != PANEL:
            return False
        return hashlib.sha256(title).hexdigest() == expected

    def erased_title_with_one_teal_pixel(rgb, mode):
        """Host-only negative: nearly empty headline that passed the old oracle."""
        left, top, right, bottom = box(mode,22,16,180,30)
        damaged = bytearray(rgb)
        row = PANEL * (right-left)
        for y in range(top,bottom):
            start = (y*w + left)*3
            damaged[start:start+len(row)] = row
        # Preserve one TEAL pixel: the previous 800px check accepted this.
        start = (top*w + left)*3
        damaged[start:start+3] = TEAL
        return bytes(damaged)


    glyphs = kernel_glyphs()
    caption = "NO TASK"
    stop_receipts = {"native_frames": 0, "erased_letter_negatives": 0}

    def stop_caption_ok(rgb, mode):
        # 800px uses 5/6 source scaling in either F6 preference; 1080p
        # Compact uses 1x. 1080p Fit uses the separate 2x font, so it is
        # intentionally excluded from this 10x20-source-only oracle.
        if not (small or mode == "compact"):
            raise AssertionError("unsupported font size for Stop oracle")
        for i, ch in enumerate(caption):
            bounds = box(mode,48+i*6,361,6,10)
            left, top, right, bottom = bounds
            if crop(rgb,w,bounds) != stop_cell_rgb(glyphs,ch,right-left,bottom-top):
                return False
        return True

    def erased_stop_letter(rgb, mode, i):
        # Deliberately damage a HOST COPY of an actually captured P6 frame,
        # leaving the original capture and other six cells unchanged.
        left, top, right, bottom = box(mode,48+i*6,361,6,10)
        damaged = bytearray(rgb)
        for y in range(top,bottom):
            offset = (y*w+left)*3
            damaged[offset:offset+(right-left)*3] = GROUND*(right-left)
        return bytes(damaged)

    def check_stop_positive_and_negatives(rgb, mode):
        assert stop_caption_ok(rgb,mode), "original native disabled Stop caption differs from source glyphs"
        checked_negatives = 0
        for i in (1,5):  # O and S, the faint characters missed by threshold-only masks
            mutated = erased_stop_letter(rgb,mode,i)
            assert mutated != rgb, "Stop adverse control did not alter actual ink"
            assert not stop_caption_ok(mutated,mode), "missing Stop caption letter was accepted"
            checked_negatives += 1
        # Counters advance only after every comparison for this captured frame succeeds.
        stop_receipts["native_frames"] += 1
        stop_receipts["erased_letter_negatives"] += checked_negatives

    def editor(rgb, mode):
        return area(rgb,mode,140,460,10,20)

    def badge(rgb, mode):
        return area(rgb,mode,418,513,72,10)

    try:
        wait_serial(f"desktop: {w}x{h} at 1x UI scale")
        first = sample("launcher",menu)
        selection = crop(first,w,(0,0,520,150))
        m.key("ret")
        wait_serial(f"starting {app.name} ({app.stat().st_size} bytes, SHA-256 {report['app_sha256']})")
        wait_serial("Tailorbird first frame presented")
        empty = sample("fit",lambda rgb: form(rgb,"fit"))
        if small: check_stop_positive_and_negatives(empty,"fit")
        m.key("a")
        typed = sample("typed",lambda rgb: form(rgb,"fit") and editor(rgb,"fit") != editor(empty,"fit"))
        if small: check_stop_positive_and_negatives(typed,"fit")
        m.key("f6")
        compact = sample("compact",lambda rgb: form(rgb,"compact") and
                         badge(rgb,"compact") != badge(typed,"fit") and
                         len(set(editor(rgb,"compact")[::3])) > 1)
        check_stop_positive_and_negatives(compact,"compact")
        if small:
            assert editor(compact,"compact") == editor(typed,"fit")
        for mode, rgb in [("fit",typed),("compact",compact)]:
            report[mode+"_title_rgb_sha256"] = hashlib.sha256(area(rgb,mode,22,16,180,30)).hexdigest()
            assert not form(erased_title_with_one_teal_pixel(rgb,mode),mode), (
                f"{mode}: erased title with one teal pixel was accepted"
            )
        m.key("f6")
        fit_again = sample("fit_again",lambda rgb: rgb == typed)
        if small: check_stop_positive_and_negatives(fit_again,"fit")
        require_stop_receipts(stop_receipts, small)
        report["disabled_stop_source_rgb"] = dict(stop_receipts)
        m.key("esc")
        wait_serial(f"{app.name} ended DONE: exit 0")
        sample("returned",lambda rgb: menu(rgb) and crop(rgb,w,(0,0,520,150)) == selection)
        m.key("p")
        assert m.proc.wait(timeout=50) == 33
        log=m.serial()
        assert re.search(re.escape(measurement) + r" \(measured; equal",log)
        assert "ALL SELF-TESTS PASS" in log and "  FAIL" not in log
        report["passed"] = True
    finally:
        m.stop()
        (folder/"result.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report),flush=True)


if __name__ == "__main__":
    selftest_stop_receipts()
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("kernel","app","output"):
        parser.add_argument(name,type=Path)
    parser.add_argument("measurement")
    args=parser.parse_args()
    for name, small in [("1080p",False),("800x600",True)]:
        check(args.kernel.resolve(), args.app.resolve(), args.measurement,
              args.output.resolve()/name, small)
