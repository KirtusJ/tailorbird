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


# Independent source-font oracles for the entire disabled STOP heading and
# NO TASK caption in every captured native view, including 1920px Fit.
# These check exact RGB glyph identity, not just contrast or ink presence.
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


def double_glyphs():
    """Read the pinned guest's actual independently rasterized 20x40 font."""
    raw = FONT.with_name("font-2x.bin").read_bytes()
    if len(raw) != 95 * 20 * 40:
        raise AssertionError("kernel double-font byte length changed")
    return [raw[i:i+800] for i in range(0, len(raw), 800)]


def stop_cell_rgb(glyphs, doubled, ch, width, height, ink, background=GROUND):
    """Separate source-exact replay of Frame::glyph's font choice and RGB blend."""
    if width < 1 or height < 1 or width > 40 or height > 60:
        raise AssertionError("outside pinned native Stop font regime")
    sw, sh = (20,40) if width > 10 or height > 20 else (10,20)
    mask = (doubled if sw == 20 else glyphs)[ord(ch) - 32]
    result = bytearray()
    for y in range(height):
        y0, y1 = y*sh//height, max((y+1)*sh//height, y*sh//height+1)
        for x in range(width):
            x0, x1 = x*sw//width, max((x+1)*sw//width, x*sw//width+1)
            alpha = sum(mask[yy*sw+xx] for yy in range(y0,y1)
                        for xx in range(x0,x1)) // ((x1-x0)*(y1-y0))
            result.extend((base*(255-alpha)+channel*alpha+127)//255
                          for base,channel in zip(background,ink))
    return bytes(result)


def require_stop_receipts(receipts, small):
    """Refuse success if any source-RGB frame or erased-letter check was skipped."""
    # Four actual captured states at each size: Fit, typed, Compact, Fit-again.
    expected = {"native_frames": 4, "erased_letter_negatives": 8,
                "heading_frames": 4, "erased_heading_negatives": 4,
                "erased_caption_negatives": 4}
    if receipts != expected:
        raise AssertionError(f"disabled Stop oracle checks incomplete: {receipts!r} != {expected!r}")


def selftest_stop_receipts():
    """Host-only missing-check controls, independent of actual QEMU frames."""
    for small in (False, True):
        expected = {"native_frames": 4, "erased_letter_negatives": 8,
                    "heading_frames": 4, "erased_heading_negatives": 4,
                    "erased_caption_negatives": 4}
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
    print("Stop receipt gate: 2 valid and 10 skipped-check adverse controls PASS", flush=True)

def require_mode_receipts(receipts):
    expected = {"native_frames": 4, "erased_heading_negatives": 4,
                "erased_state_negatives": 4}
    if receipts != expected:
        raise AssertionError(f"native MODE optical checks incomplete: {receipts!r} != {expected!r}")


def selftest_mode_receipts():
    expected = {"native_frames": 4, "erased_heading_negatives": 4,
                "erased_state_negatives": 4}
    require_mode_receipts(dict(expected))
    for key in expected:
        missing = dict(expected)
        missing[key] -= 1
        try:
            require_mode_receipts(missing)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"missing MODE check not refused: {key}")
    print("MODE receipt gate: 1 valid and 3 missing-check controls PASS", flush=True)


def require_f6_receipts(receipts):
    expected = {"native_frames": 4, "erased_badge_negatives": 4}
    if receipts != expected:
        raise AssertionError(f"native F6 badge checks incomplete: {receipts!r} != {expected!r}")


def selftest_f6_receipts():
    expected = {"native_frames": 4, "erased_badge_negatives": 4}
    require_f6_receipts(dict(expected))
    for key in expected:
        missing = dict(expected)
        missing[key] -= 1
        try:
            require_f6_receipts(missing)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"missing F6 check not refused: {key}")
    print("F6 receipt gate: 1 valid and 2 missing-check controls PASS", flush=True)


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
    doubled = double_glyphs()
    caption = "NO TASK"
    stop_receipts = {"native_frames": 0, "erased_letter_negatives": 0,
                     "heading_frames": 0, "erased_heading_negatives": 0,
                     "erased_caption_negatives": 0}
    mode_receipts = {"native_frames": 0, "erased_heading_negatives": 0,
                     "erased_state_negatives": 0}
    f6_receipts = {"native_frames": 0, "erased_badge_negatives": 0}

    def stop_text_ok(rgb, mode, text, x, y, step, cell_width, cell_height, ink, background=GROUND):
        for i, ch in enumerate(text):
            bounds = box(mode,x+i*step,y,cell_width,cell_height)
            left, top, right, bottom = bounds
            expected = stop_cell_rgb(glyphs,doubled,ch,right-left,bottom-top,ink,background)
            if crop(rgb,w,bounds) != expected:
                return False
        return True

    def stop_caption_ok(rgb, mode):
        # Includes the 1920px Fit 2x font; old tests only checked Compact.
        return stop_text_ok(rgb,mode,caption,48,361,6,6,10,SOFT)

    def stop_heading_ok(rgb, mode):
        # The STOP heading uses the independent double-size source font at
        # 800px, 1080p Fit and 1080p Compact, with exact per-cell bounds.
        return stop_text_ok(rgb,mode,"STOP",48,331,18,18,30,TEXT)

    def erased_stop_cell(rgb, mode, i):
        # Deliberately damage only a HOST COPY of an original captured P6.
        left, top, right, bottom = box(mode,48+i*6,361,6,10)
        damaged = bytearray(rgb)
        for y in range(top,bottom):
            offset = (y*w+left)*3
            damaged[offset:offset+(right-left)*3] = GROUND*(right-left)
        return bytes(damaged)

    def erased_stop_text(rgb, mode, heading):
        text, y, step, cw, ch = (("STOP",331,18,18,30) if heading
                                 else (caption,361,6,6,10))
        damaged = bytearray(rgb)
        for i in range(len(text)):
            left, top, right, bottom = box(mode,48+i*step,y,cw,ch)
            for row in range(top,bottom):
                offset = (row*w+left)*3
                damaged[offset:offset+(right-left)*3] = GROUND*(right-left)
        return bytes(damaged)

    def check_stop_positive_and_negatives(rgb, mode):
        assert stop_caption_ok(rgb,mode), "native disabled Stop caption differs from source glyphs"
        assert stop_heading_ok(rgb,mode), "native STOP heading differs from source glyphs"
        checked_negatives = 0
        for i in (1,5):  # O and S, the faint characters missed by threshold-only masks
            mutated = erased_stop_cell(rgb,mode,i)
            assert mutated != rgb, "Stop caption adverse control did not alter actual ink"
            assert not stop_caption_ok(mutated,mode), "missing Stop caption letter was accepted"
            checked_negatives += 1
        erased_caption = erased_stop_text(rgb,mode,False)
        assert erased_caption != rgb, "erased Stop caption control was empty"
        assert not stop_caption_ok(erased_caption,mode), "missing whole Stop caption accepted"
        erased_heading = erased_stop_text(rgb,mode,True)
        assert erased_heading != rgb, "erased STOP heading control was empty"
        assert not stop_heading_ok(erased_heading,mode), "missing STOP heading accepted"
        # Only record a check after its original and all host-negative comparisons pass.
        stop_receipts["native_frames"] += 1
        stop_receipts["erased_letter_negatives"] += checked_negatives
        stop_receipts["heading_frames"] += 1
        stop_receipts["erased_heading_negatives"] += 1
        stop_receipts["erased_caption_negatives"] += 1


    def mode_heading_ok(rgb, mode):
        # Heading uses PANEL background, while the button state uses GROUND.
        return stop_text_ok(rgb,mode,"MODE",38,187,12,12,20,TEAL,PANEL)

    def mode_state_ok(rgb, mode):
        # Offline native guest begins in SINGLE REQUEST; F6 changes only view.
        return stop_text_ok(rgb,mode,"SINGLE REQUEST",48,232,12,12,20,TEXT)

    def erased_mode_text(rgb, mode, heading):
        text, x, y, background = (("MODE",38,187,PANEL) if heading
                                  else ("SINGLE REQUEST",48,232,GROUND))
        damaged = bytearray(rgb)
        for i in range(len(text)):
            left, top, right, bottom = box(mode,x+i*12,y,12,20)
            for row in range(top,bottom):
                offset = (row*w+left)*3
                damaged[offset:offset+(right-left)*3] = background*(right-left)
        return bytes(damaged)

    def check_mode_positive_and_negatives(rgb, mode):
        assert mode_heading_ok(rgb,mode), "native MODE heading differs from source glyphs"
        assert mode_state_ok(rgb,mode), "native mode-state text differs from source glyphs"
        erased_heading = erased_mode_text(rgb,mode,True)
        assert erased_heading != rgb, "MODE heading adverse control did not alter native ink"
        assert not mode_heading_ok(erased_heading,mode), "erased MODE heading accepted"
        erased_state = erased_mode_text(rgb,mode,False)
        assert erased_state != rgb, "mode-state adverse control did not alter native ink"
        assert not mode_state_ok(erased_state,mode), "erased mode state accepted"
        mode_receipts["native_frames"] += 1
        mode_receipts["erased_heading_negatives"] += 1
        mode_receipts["erased_state_negatives"] += 1

    def f6_badge_ok(rgb, mode):
        # Source: guest/main.rs paints exact F6: FIT or F6: COMPACT in TEAL
        # at logical (418, 513), using Frame::text's 6x10 single-scale glyphs.
        label = "F6: COMPACT" if mode == "compact" else "F6: FIT"
        return stop_text_ok(rgb, mode, label, 418, 513, 6, 6, 10, TEAL)

    def erased_f6_badge(rgb, mode):
        # Damage a HOST COPY. Existing cross-view badge inequality remains
        # satisfied by an entirely erased Compact badge in original frames.
        label = "F6: COMPACT" if mode == "compact" else "F6: FIT"
        damaged = bytearray(rgb)
        for i in range(len(label)):
            left, top, right, bottom = box(mode, 418+i*6, 513, 6, 10)
            for row in range(top, bottom):
                offset = (row*w + left)*3
                damaged[offset:offset+(right-left)*3] = GROUND*(right-left)
        return bytes(damaged)

    def check_f6_positive_and_negative(rgb, mode):
        assert f6_badge_ok(rgb, mode), "native F6 badge differs from source glyphs"
        erased = erased_f6_badge(rgb, mode)
        assert erased != rgb, "erased F6 badge did not alter native ink"
        assert not f6_badge_ok(erased, mode), "missing F6 badge was accepted"
        f6_receipts["native_frames"] += 1
        f6_receipts["erased_badge_negatives"] += 1

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
        check_stop_positive_and_negatives(empty,"fit")
        check_mode_positive_and_negatives(empty,"fit")
        check_f6_positive_and_negative(empty,"fit")
        m.key("a")
        typed = sample("typed",lambda rgb: form(rgb,"fit") and editor(rgb,"fit") != editor(empty,"fit"))
        check_stop_positive_and_negatives(typed,"fit")
        check_mode_positive_and_negatives(typed,"fit")
        check_f6_positive_and_negative(typed,"fit")
        m.key("f6")
        compact = sample("compact",lambda rgb: form(rgb,"compact") and
                         badge(rgb,"compact") != badge(typed,"fit") and
                         len(set(editor(rgb,"compact")[::3])) > 1)
        check_stop_positive_and_negatives(compact,"compact")
        check_mode_positive_and_negatives(compact,"compact")
        check_f6_positive_and_negative(compact,"compact")
        if small:
            assert editor(compact,"compact") == editor(typed,"fit")
        for mode, rgb in [("fit",typed),("compact",compact)]:
            report[mode+"_title_rgb_sha256"] = hashlib.sha256(area(rgb,mode,22,16,180,30)).hexdigest()
            assert not form(erased_title_with_one_teal_pixel(rgb,mode),mode), (
                f"{mode}: erased title with one teal pixel was accepted"
            )
        m.key("f6")
        fit_again = sample("fit_again",lambda rgb: rgb == typed)
        check_stop_positive_and_negatives(fit_again,"fit")
        check_mode_positive_and_negatives(fit_again,"fit")
        check_f6_positive_and_negative(fit_again,"fit")
        require_stop_receipts(stop_receipts, small)
        require_mode_receipts(mode_receipts)
        require_f6_receipts(f6_receipts)
        report["f6_badge_source_rgb"] = dict(f6_receipts)
        report["disabled_stop_source_rgb"] = dict(stop_receipts)
        report["mode_source_rgb"] = dict(mode_receipts)
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
    selftest_mode_receipts()
    selftest_f6_receipts()
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("kernel","app","output"):
        parser.add_argument(name,type=Path)
    parser.add_argument("measurement")
    args=parser.parse_args()
    for name, small in [("1080p",False),("800x600",True)]:
        check(args.kernel.resolve(), args.app.resolve(), args.measurement,
              args.output.resolve()/name, small)
