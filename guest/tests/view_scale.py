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
BAD = bytes.fromhex("f08a72")
GOOD = bytes.fromhex("8fe0c2")
AMBER = bytes.fromhex("e3a24b")


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


def require_offline_status_receipts(receipts):
    # Every native frame must prove all three actual offline identity labels
    # and each host-side whole-label erasure must be refused.
    expected = {"native_frames": 4, "erased_label_negatives": 12}
    if receipts != expected:
        raise AssertionError(f"offline status oracle incomplete: {receipts!r} != {expected!r}")


def selftest_offline_status_receipts():
    complete = {"native_frames": 4, "erased_label_negatives": 12}
    require_offline_status_receipts(dict(complete))
    for key in complete:
        damaged = dict(complete)
        damaged[key] -= 1
        try:
            require_offline_status_receipts(damaged)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"missing offline status check accepted: {key}")
    print("Offline-status receipt gate: 1 valid and 2 skipped-check controls PASS", flush=True)

def require_editor_receipts(receipts):
    expected = {"native_frames": 4, "case_negatives": 4,
                "count_negatives": 3}
    if receipts != expected:
        raise AssertionError(f"native request-editor oracle incomplete: {receipts!r} != {expected!r}")


def selftest_editor_receipts():
    complete = {"native_frames": 4, "case_negatives": 4,
                "count_negatives": 3}
    require_editor_receipts(dict(complete))
    for key in complete:
        missing = dict(complete)
        missing[key] -= 1
        try:
            require_editor_receipts(missing)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"missing native editor check accepted: {key}")
    print("Editor receipt gate: 1 valid and 3 missing-check controls PASS", flush=True)


def require_shortcut_legend_receipts(receipts):
    expected = {"native_frames": 4, "erased_stop_negatives": 4,
                "erased_exit_negatives": 4}
    if receipts != expected:
        raise AssertionError(
            f"native keyboard legend oracle incomplete: {receipts!r} != {expected!r}"
        )


def selftest_shortcut_legend_receipts():
    complete = {"native_frames": 4, "erased_stop_negatives": 4,
                "erased_exit_negatives": 4}
    require_shortcut_legend_receipts(dict(complete))
    for key in complete:
        missing = dict(complete)
        missing[key] -= 1
        try:
            require_shortcut_legend_receipts(missing)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"missing keyboard legend check accepted: {key}")
    print("Keyboard legend receipt gate: 1 valid and 3 missing-check controls PASS",
          flush=True)



def require_empty_output_receipts(receipts):
    # The local offline preview must disclose its empty event feed and keep
    # manual OS / Escape navigation legible at both tested resolutions.
    expected = {"native_frames": 4, "erased_guidance_negatives": 12}
    if receipts != expected:
        raise AssertionError(f"empty-output guidance checks incomplete: {receipts!r} != {expected!r}")


def selftest_empty_output_receipts():
    complete = {"native_frames": 4, "erased_guidance_negatives": 12}
    require_empty_output_receipts(dict(complete))
    for key in complete:
        missing = dict(complete)
        missing[key] -= 1
        try:
            require_empty_output_receipts(missing)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"missing empty-output check accepted: {key}")
    print("Empty-output guidance receipts: 1 valid, 2 skipped-check refusals PASS",
          flush=True)


def require_task_section_receipts(receipts):
    expected = {"native_frames": 4, "erased_label_negatives": 16}
    if receipts != expected:
        raise AssertionError(
            f"native task-section label checks incomplete: {receipts!r} != {expected!r}"
        )


def selftest_task_section_receipts():
    complete = {"native_frames": 4, "erased_label_negatives": 16}
    require_task_section_receipts(dict(complete))
    for key in complete:
        missing = dict(complete)
        missing[key] -= 1
        try:
            require_task_section_receipts(missing)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"missing task-section check accepted: {key}")
    print("Task-section receipt gate: 1 valid, 2 skipped-check refusals PASS",
          flush=True)


def require_affordance_receipts(receipts):
    # The local mode action and editor-capacity disclosures must each be
    # source-RGB checked and adversarially erased in four true guest states.
    expected = {"native_frames": 4, "erased_affordance_negatives": 16}
    if receipts != expected:
        raise AssertionError(
            f"native mode/request affordance checks incomplete: {receipts!r} != {expected!r}"
        )


def selftest_affordance_receipts():
    complete = {"native_frames": 4, "erased_affordance_negatives": 16}
    require_affordance_receipts(dict(complete))
    for key in complete:
        missing = dict(complete)
        missing[key] -= 1
        try:
            require_affordance_receipts(missing)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"missing native affordance check accepted: {key}")
    print("Mode/request affordance receipt gate: 1 valid, 2 missing-check refusals PASS",
          flush=True)



def require_offline_refusal_receipts(receipts):
    expected = {"native_frames": 3, "erased_notice_negatives": 3,
                "unchanged_other_pixels": 3}
    if receipts != expected:
        raise AssertionError(f"offline action/refusal checks incomplete: {receipts!r}")


def selftest_offline_refusal_receipts():
    complete = {"native_frames": 3, "erased_notice_negatives": 3,
                "unchanged_other_pixels": 3}
    require_offline_refusal_receipts(dict(complete))
    for key in complete:
        missing = dict(complete)
        missing[key] -= 1
        try:
            require_offline_refusal_receipts(missing)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"skipped offline refusal check accepted: {key}")
    print("Offline refusal receipts: 1 valid, 3 skipped-check refusals PASS",
          flush=True)



def require_launcher_return_receipts(receipts):
    expected = {"original_frames": 1, "stale_app_negatives": 1,
                "tampered_status_negatives": 1, "tampered_selection_negatives": 1,
                "tampered_status_margin_negatives": 2}
    if receipts != expected:
        raise AssertionError(f"full-launcher return checks incomplete: {receipts!r}")


def selftest_launcher_return_receipts():
    complete = {"original_frames": 1, "stale_app_negatives": 1,
                "tampered_status_negatives": 1, "tampered_selection_negatives": 1,
                "tampered_status_margin_negatives": 2}
    require_launcher_return_receipts(dict(complete))
    for key in complete:
        wrong = dict(complete)
        wrong[key] -= 1
        try:
            require_launcher_return_receipts(wrong)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"skipped launcher return oracle accepted: {key}")
    print("Full-launcher return receipts: 1 valid and 5 missing-check refusals PASS",
          flush=True)


def launcher_dynamic_row(y, height):
    # #289 CI builds the desktop kernel pinned in the workflow at
    # 8f5992807e9b9c0bb8ce64e366c85ee4add31599, not the main repo's
    # old text-mode menu. draw_desktop paints ONLY the first result line
    # at ui::at(2, rows-4), with 10x20 glyph cells.
    row = height // 20 - 4
    return row * 20 <= y < (row + 1) * 20


def launcher_static_same(first, returned, width, height, line):
    """Only the exact status-glyph cells may differ from the initial desktop.

    The pinned draw_desktop uses ui::at(2, rows-4) with 10x20 glyph
    cells. Every *other* pixel, including both margins of that same
    text row, must be byte-identical to this launch's original frame.
    """
    if (line is None or len(first) != width * height * 3 or
            len(returned) != len(first)):
        return False
    x0, x1 = 20, 20 + len(line) * 10
    y0 = (height // 20 - 4) * 20
    if x1 > width or y0 < 0 or y0 + 20 > height:
        return False
    stride = width * 3
    left, right = x0 * 3, x1 * 3
    for y in range(height):
        start = y * stride
        if y0 <= y < y0 + 20:
            if (first[start:start+left] != returned[start:start+left] or
                    first[start+right:start+stride] !=
                    returned[start+right:start+stride]):
                return False
        elif first[start:start+stride] != returned[start:start+stride]:
            return False
    return True


def launcher_status_line(serial, name):
    # Pinned desktop kernel's run_entry message, authenticated to this
    # launch by its serial log, not to a screenshot from another attempt.
    # draw_desktop displays only message.lines().next().
    result = re.search(re.escape(name) +
                       r" ended DONE: exit 0 after (\d+) s;", serial)
    if result is None:
        return None
    return f"{name} ended DONE: exit 0 after {result.group(1)} s."


def launcher_status_source_ok(rgb, width, height, glyphs, doubled, line):
    if line is None:
        return False
    # draw_desktop's ui::at(2, rows-4), AMBER on GROUND, 10x20 cells.
    # The status text fits both native tested widths without clipping.
    y = (height // 20 - 4) * 20
    if len(line) * 10 + 20 > width:
        return False
    for j, ch in enumerate(line):
        expected = stop_cell_rgb(glyphs, doubled, ch, 10, 20,
                                 AMBER, GROUND)
        if crop(rgb, width, (20 + j*10, y, 30 + j*10, y + 20)) != expected:
            return False
    return True


def full_launcher_return_ok(before, after, width, height,
                            serial, name, glyphs, doubled):
    line = launcher_status_line(serial, name)
    return (launcher_static_same(before, after, width, height, line) and
            launcher_status_source_ok(after, width, height, glyphs, doubled,
                                      line))


def transplanted_stale_app(returned, app_frame, width, height):
    # Disposable host-only adverse image. Preserve genuine dynamic status
    # while transplanting stale Tailorbird into the rest of the launcher.
    changed = bytearray(returned)
    stride = width * 3
    for y in range(height):
        if not launcher_dynamic_row(y, height):
            start = y*stride
            changed[start:start+stride] = app_frame[start:start+stride]
    return bytes(changed)


def erased_return_status(returned, width, height):
    # Remove the first 10x20 glyph from the genuine returned result line.
    changed = bytearray(returned)
    row_y = (height // 20 - 4) * 20
    for y in range(row_y, row_y + 20):
        start = (y*width + 20)*3
        changed[start:start+30] = GROUND*10
    return bytes(changed)


def damaged_launcher_selection(returned, width):
    changed = bytearray(returned)
    start = (110*width+21)*3
    changed[start:start+3] = GROUND
    return bytes(changed)


def tampered_status_margin(returned, width, height, at_right):
    """Flip one channel in the exempt row *outside* all status glyph cells."""
    x = width - 1 if at_right else 0
    y = (height // 20 - 4) * 20 + 10
    changed = bytearray(returned)
    changed[(y * width + x) * 3] ^= 0xff
    return bytes(changed)


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
    offline_refusal_receipts = {"native_frames": 0, "erased_notice_negatives": 0,
                               "unchanged_other_pixels": 0}
    stop_receipts = {"native_frames": 0, "erased_letter_negatives": 0,
                     "heading_frames": 0, "erased_heading_negatives": 0,
                     "erased_caption_negatives": 0}
    mode_receipts = {"native_frames": 0, "erased_heading_negatives": 0,
                     "erased_state_negatives": 0}
    f6_receipts = {"native_frames": 0, "erased_badge_negatives": 0}
    offline_receipts = {"native_frames": 0, "erased_label_negatives": 0}
    editor_receipts = {"native_frames": 0, "case_negatives": 0,
                       "count_negatives": 0}
    shortcut_receipts = {"native_frames": 0, "erased_stop_negatives": 0,
                         "erased_exit_negatives": 0}
    empty_output_receipts = {"native_frames": 0, "erased_guidance_negatives": 0}
    task_section_receipts = {"native_frames": 0, "erased_label_negatives": 0}
    affordance_receipts = {"native_frames": 0, "erased_affordance_negatives": 0}

    def stop_text_ok(rgb, mode, text, x, y, step, cell_width, cell_height, ink, background=GROUND):
        for i, ch in enumerate(text):
            bounds = box(mode,x+i*step,y,cell_width,cell_height)
            left, top, right, bottom = bounds
            expected = stop_cell_rgb(glyphs,doubled,ch,right-left,bottom-top,ink,background)
            if crop(rgb,w,bounds) != expected:
                return False
        return True


    # The public native preview has no service. These are real keyboard
    # refusal/availability notices, not trusted service acknowledgements.
    # The remaining 70-cell footer lane is x=535..955, on GROUND.
    def offline_notice_ok(rgb, mode, value):
        if len(value) > 70 or value != value.upper():
            raise AssertionError("outside pinned notice-font admission")
        return stop_text_ok(rgb, mode, value.ljust(70), 535, 513,
                            6, 6, 10, AMBER, GROUND)

    def blank_notice_region(rgb, mode):
        # Make only a disposable host copy with the entire notice lane blank.
        left, top, right, bottom = box(mode, 535, 513, 420, 10)
        damaged = bytearray(rgb)
        for yy in range(top, bottom):
            offset = (yy*w+left)*3
            damaged[offset:offset+(right-left)*3] = GROUND*(right-left)
        return bytes(damaged)

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


    # Pinned native offline preview at this exact guest revision: Ui::new()
    # starts connected=false, Model::Unknown. This is a display identity
    # check, not a claim about a trusted service or an authenticated model.
    offline_labels = (
        ("NO SERVICE", 737, 27, BAD),
        ("CONNECTION", 38, 115, TEAL),
        ("NOT CONNECTED", 38, 147, AMBER),
    )

    def offline_label_ok(rgb, mode, label, x, y, ink):
        # The first label occupies the top PANEL; the latter two the
        # left-sidebar PANEL. All use the actual guest Frame::text 2x cells.
        return stop_text_ok(rgb, mode, label, x, y, 12, 12, 20, ink, PANEL)

    def erased_offline_label(rgb, mode, label, x, y):
        # Host-only mutation of an original native P6; never modify the
        # guest or pretend this adverse frame was produced by QEMU.
        altered = bytearray(rgb)
        for i in range(len(label)):
            left, top, right, bottom = box(mode, x+i*12, y, 12, 20)
            for yy in range(top, bottom):
                offset = (yy*w + left)*3
                altered[offset:offset+(right-left)*3] = PANEL*(right-left)
        return bytes(altered)

    def check_offline_positive_and_negatives(rgb, mode):
        for label, x, y, ink in offline_labels:
            assert offline_label_ok(rgb, mode, label, x, y, ink), (
                f"native offline status {label!r} differs from pinned source RGB"
            )
            erased = erased_offline_label(rgb, mode, label, x, y)
            assert erased != rgb, f"offline status {label!r} erasure removed no ink"
            assert not offline_label_ok(erased, mode, label, x, y, ink), (
                f"erased offline status {label!r} accepted"
            )
            offline_receipts["erased_label_negatives"] += 1
        offline_receipts["native_frames"] += 1


    # The native preview types lower-case 'a'. Source-identity checks must
    # distinguish that byte from uppercase 'A', as well as 0 versus 1 BYTES.
    # Mere changed-pixel presence in editor() cannot establish either fact.
    def editor_glyph_ok(rgb, mode, character):
        return stop_text_ok(rgb, mode, character, 140, 460, 10, 10, 20, TEXT, PANEL)

    def editor_count_ok(rgb, mode, digit):
        return stop_text_ok(rgb, mode, digit, 70, 489, 6, 6, 10, TEXT, PANEL)

    def substitute_editor_cell(rgb, mode, x, y, cell_w, cell_h, character):
        left, top, right, bottom = box(mode, x, y, cell_w, cell_h)
        expected = stop_cell_rgb(glyphs, doubled, character,
                                 right-left, bottom-top, TEXT, PANEL)
        amended = bytearray(rgb)
        cell_w_rgb = (right-left)*3
        for yy in range(top, bottom):
            out = (yy*w+left)*3
            offset = (yy-top)*cell_w_rgb
            amended[out:out+cell_w_rgb] = expected[offset:offset+cell_w_rgb]
        return bytes(amended)

    def check_editor_positive_and_negatives(rgb, mode, character, digit):
        assert editor_glyph_ok(rgb, mode, character), (
            f"native editor {character!r} differs from case-sensitive kernel glyph"
        )
        assert editor_count_ok(rgb, mode, digit), (
            f"native editor BYTES {digit!r} differs from kernel glyph"
        )
        # Host-only injection into the empty editor, or case flip of typed 'a'.
        injected = substitute_editor_cell(rgb, mode, 140, 460, 10, 20, "A")
        assert injected != rgb, "editor adverse case control did not change pixels"
        assert not editor_glyph_ok(injected, mode, character), (
            "wrong-case/nonempty editor host copy accepted"
        )
        editor_receipts["case_negatives"] += 1
        if digit == "1":
            reset = substitute_editor_cell(rgb, mode, 70, 489, 6, 10, "0")
            assert reset != rgb, "BYTES reset adverse control did not change pixels"
            assert not editor_count_ok(reset, mode, digit), (
                "wrong request byte count host copy accepted"
            )
            editor_receipts["count_negatives"] += 1
        editor_receipts["native_frames"] += 1

    # This keyboard legend describes the visible normal-mode Stop and
    # launcher-return shortcuts. Full source RGB proves the displayed words,
    # NOT whether trusted service or kernel actually admits those commands.
    shortcut_legend = (
        "ENTER SUBMIT   ALT R RETRY   ALT M MODE   ALT S STOP   ESC EXIT"
    )

    def shortcut_legend_ok(rgb, mode):
        return stop_text_ok(rgb, mode, shortcut_legend, 22, 513,
                            6, 6, 10, SOFT)

    def erased_shortcut(rgb, mode, token):
        # Host-only negative: erase one entire safety-relevant shortcut while
        # preserving the original QEMU capture and all neighboring badges.
        assert shortcut_legend.count(token) == 1
        start = shortcut_legend.index(token)
        altered = bytearray(rgb)
        for i in range(len(token)):
            left, top, right, bottom = box(mode, 22+(start+i)*6, 513, 6, 10)
            for yy in range(top, bottom):
                offset = (yy*w+left)*3
                altered[offset:offset+(right-left)*3] = GROUND*(right-left)
        return bytes(altered)

    def check_shortcut_legend(rgb, mode):
        assert shortcut_legend_ok(rgb, mode), (
            "native footer Stop/Exit keyboard legend differs from exact source RGB"
        )
        for token, key in (("ALT S STOP", "erased_stop_negatives"),
                           ("ESC EXIT", "erased_exit_negatives")):
            changed = erased_shortcut(rgb, mode, token)
            assert changed != rgb, f"{token} erasure removed no real native ink"
            assert not shortcut_legend_ok(changed, mode), (
                f"missing {token} legend incorrectly accepted"
            )
            shortcut_receipts[key] += 1
        shortcut_receipts["native_frames"] += 1


    # The native model-free Ui::new() has no task-event rows. The three
    # genuinely painted guidance lines live inside the x=316..940 PANEL,
    # with no permission inspector. An empty/misleading output pane used
    # to pass even with valid title, Stop, F6 and editor pixels.
    empty_output_labels = (
        ("NO TASK EVENTS RECEIVED", 339, 205, 12, 20, SOFT),
        ("MANUAL OS USE REMAINS AVAILABLE", 339, 246, 6, 10, GOOD),
        ("ESC RETURNS TO THE PCP OS LAUNCHER", 339, 270, 6, 10, SOFT),
    )

    def empty_output_label_ok(rgb, mode, text, x, y, cell_w, cell_h, ink):
        return stop_text_ok(rgb, mode, text, x, y,
                            cell_w, cell_w, cell_h, ink, PANEL)

    def erased_empty_output_label(rgb, mode, text, x, y, cell_w, cell_h):
        # Deliberate host-only adverse copy, never alter a native P6 capture.
        damaged = bytearray(rgb)
        for i in range(len(text)):
            left, top, right, bottom = box(mode, x+i*cell_w, y,
                                           cell_w, cell_h)
            for yy in range(top, bottom):
                offset = (yy*w+left)*3
                damaged[offset:offset+(right-left)*3] = PANEL*(right-left)
        return bytes(damaged)

    def check_empty_output_positive_and_negatives(rgb, mode):
        for text, x, y, cw, ch, ink in empty_output_labels:
            assert empty_output_label_ok(rgb, mode, text, x, y, cw, ch, ink), (
                f"native empty-output guidance {text!r} differs from source RGB"
            )
            erased = erased_empty_output_label(rgb, mode, text, x, y, cw, ch)
            assert erased != rgb, f"no actual native ink removed for {text!r}"
            assert not empty_output_label_ok(erased, mode, text, x, y, cw, ch, ink), (
                f"erased empty-output guidance {text!r} was accepted"
            )
            empty_output_receipts["erased_guidance_negatives"] += 1
        empty_output_receipts["native_frames"] += 1

    # Four distinct live headings and Stop-key disclosure fields. Source
    # typeface and exact positions are pinned to guest/main.rs.
    task_section_labels = (
        ("LOCAL TASK SURFACE", 260, 23, 12, 20, TEXT),
        ("TASK OUTPUT", 335, 116, 12, 20, TEAL),
        ("NO TASK", 337, 150, 6, 10, SOFT),
        ("ALT S STOP TASK", 38, 408, 6, 10, SOFT),
    )

    def task_section_label_ok(rgb, mode, text, x, y, cell_w, cell_h, ink):
        return stop_text_ok(rgb, mode, text, x, y,
                            cell_w, cell_w, cell_h, ink, PANEL)

    def check_task_section_positive_and_negatives(rgb, mode):
        for text, x, y, cw, ch, ink in task_section_labels:
            assert task_section_label_ok(rgb, mode, text, x, y, cw, ch, ink), (
                f"native task-section label {text!r} differs from source RGB"
            )
            # Change only a disposable host copy, never original native P6.
            erased = erased_empty_output_label(rgb, mode, text, x, y, cw, ch)
            assert erased != rgb, f"no native ink removed for task label {text!r}"
            assert not task_section_label_ok(erased, mode, text, x, y, cw, ch, ink), (
                f"erased task-section label {text!r} accepted"
            )
            task_section_receipts["erased_label_negatives"] += 1
        task_section_receipts["native_frames"] += 1

    # These four already-painted native UI disclosures are separate from the
    # previously guarded MODE state, F6 badge, request count and footer legend.
    # In particular, the actual mode-change command and 120-byte capacity
    # must remain legible even while the model and service are unavailable.
    affordance_labels = (
        ("ALT M CHANGE MODE", 40, 287, SOFT),
        ("REQUEST", 32, 469, TEAL),
        ("BYTES", 32, 489, TEAL),
        ("/120", 102, 489, SOFT),
    )

    def affordance_label_ok(rgb, mode, label, x, y, ink):
        return stop_text_ok(rgb, mode, label, x, y, 6, 6, 10, ink, PANEL)

    def check_affordance_positive_and_negatives(rgb, mode):
        for label, x, y, ink in affordance_labels:
            assert affordance_label_ok(rgb, mode, label, x, y, ink), (
                f"native mode/request affordance {label!r} differs from source RGB"
            )
            # Change only a host copy of the native screenshot; do not alter
            # the original guest P6 or assert that QEMU rendered the damage.
            erased = erased_empty_output_label(rgb, mode, label, x, y, 6, 10)
            assert erased != rgb, f"no real native ink removed for {label!r}"
            assert not affordance_label_ok(erased, mode, label, x, y, ink), (
                f"erased native mode/request affordance {label!r} accepted"
            )
            affordance_receipts["erased_affordance_negatives"] += 1
        affordance_receipts["native_frames"] += 1

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
        check_offline_positive_and_negatives(empty,"fit")
        check_editor_positive_and_negatives(empty,"fit"," ","0")
        check_shortcut_legend(empty,"fit")
        check_empty_output_positive_and_negatives(empty,"fit")
        check_task_section_positive_and_negatives(empty,"fit")
        check_affordance_positive_and_negatives(empty,"fit")
        m.key("a")
        typed = sample("typed",lambda rgb: form(rgb,"fit") and editor(rgb,"fit") != editor(empty,"fit"))
        check_stop_positive_and_negatives(typed,"fit")
        check_mode_positive_and_negatives(typed,"fit")
        check_f6_positive_and_negative(typed,"fit")
        check_offline_positive_and_negatives(typed,"fit")
        check_editor_positive_and_negatives(typed,"fit","a","1")
        check_shortcut_legend(typed,"fit")
        check_empty_output_positive_and_negatives(typed,"fit")
        check_task_section_positive_and_negatives(typed,"fit")
        check_affordance_positive_and_negatives(typed,"fit")
        m.key("f6")
        compact = sample("compact",lambda rgb: form(rgb,"compact") and
                         badge(rgb,"compact") != badge(typed,"fit") and
                         len(set(editor(rgb,"compact")[::3])) > 1)
        check_stop_positive_and_negatives(compact,"compact")
        check_mode_positive_and_negatives(compact,"compact")
        check_f6_positive_and_negative(compact,"compact")
        check_offline_positive_and_negatives(compact,"compact")
        check_editor_positive_and_negatives(compact,"compact","a","1")
        check_shortcut_legend(compact,"compact")
        check_empty_output_positive_and_negatives(compact,"compact")
        check_task_section_positive_and_negatives(compact,"compact")
        check_affordance_positive_and_negatives(compact,"compact")
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
        check_offline_positive_and_negatives(fit_again,"fit")
        check_editor_positive_and_negatives(fit_again,"fit","a","1")
        check_shortcut_legend(fit_again,"fit")
        check_empty_output_positive_and_negatives(fit_again,"fit")
        check_task_section_positive_and_negatives(fit_again,"fit")
        check_affordance_positive_and_negatives(fit_again,"fit")
        require_stop_receipts(stop_receipts, small)
        require_mode_receipts(mode_receipts)
        require_f6_receipts(f6_receipts)
        require_offline_status_receipts(offline_receipts)
        require_editor_receipts(editor_receipts)
        require_shortcut_legend_receipts(shortcut_receipts)
        require_empty_output_receipts(empty_output_receipts)
        require_task_section_receipts(task_section_receipts)
        require_affordance_receipts(affordance_receipts)
        report["f6_badge_source_rgb"] = dict(f6_receipts)
        report["offline_status_source_rgb"] = dict(offline_receipts)
        report["editor_source_rgb"] = dict(editor_receipts)
        report["keyboard_shortcut_legend_source_rgb"] = dict(shortcut_receipts)
        report["empty_output_guidance_source_rgb"] = dict(empty_output_receipts)
        report["task_section_source_rgb"] = dict(task_section_receipts)
        report["mode_request_affordance_source_rgb"] = dict(affordance_receipts)
        report["disabled_stop_source_rgb"] = dict(stop_receipts)
        report["mode_source_rgb"] = dict(mode_receipts)

        # A new, distinct native Q35 negative/positive family. Exercise the
        # *real* disconnected guest's Enter, Alt+M and Alt+S, then demand
        # source-exact complete notice glyphs and unchanged other pixels.
        # These are refusal notices, never evidence of authorized effects.
        original_without_notice = blank_notice_region(fit_again, "fit")
        for tag, chord, value in (
            ("offline_submit_refusal", ("ret",),
             "NO MODEL SERVICE - OS STILL USABLE"),
            ("offline_mode_refusal", ("alt", "m"),
             "MODE CHANGE UNAVAILABLE"),
            ("offline_stop_refusal", ("alt", "s"),
             "NO STOPPABLE TASK - USE OS STOP KEY"),
        ):
            m.key(*chord)
            observed = sample(tag, lambda rgb: offline_notice_ok(rgb, "fit", value))
            assert form(observed, "fit"), f"{tag}: title changed after refused action"
            assert editor(observed, "fit") == editor(typed, "fit"), (
                f"{tag}: unsent request editor changed after refused action"
            )
            assert blank_notice_region(observed, "fit") == original_without_notice, (
                f"{tag}: pixels outside notice changed in disconnected native guest"
            )
            erased = blank_notice_region(observed, "fit")
            assert erased != observed, f"{tag}: no real notice glyph ink"
            assert not offline_notice_ok(erased, "fit", value), (
                f"{tag}: erased refusal notice incorrectly accepted"
            )
            offline_refusal_receipts["native_frames"] += 1
            offline_refusal_receipts["erased_notice_negatives"] += 1
            offline_refusal_receipts["unchanged_other_pixels"] += 1
        require_offline_refusal_receipts(offline_refusal_receipts)
        report["offline_refusal_notice_source_rgb"] = dict(offline_refusal_receipts)
        m.key("esc")
        wait_serial(f"{app.name} ended DONE: exit 0")
        # Require byte-identical same-launch launcher pixels outside the
        # exact source-defined first status-line glyph cells, and source-RGB
        # rendering of the serial-derived actual result inside those cells.
        returned = sample("returned", lambda rgb: menu(rgb) and
                          crop(rgb,w,(0,0,520,150)) == selection and
                          full_launcher_return_ok(first, rgb, w, h,
                                                  m.serial(), app.name,
                                                  glyphs, doubled))
        return_receipts = {"original_frames": 0, "stale_app_negatives": 0,
                           "tampered_status_negatives": 0,
                           "tampered_selection_negatives": 0,
                           "tampered_status_margin_negatives": 0}
        return_receipts["original_frames"] += 1
        stale = transplanted_stale_app(returned, fit_again, w, h)
        assert stale != returned, "stale app transplant changed no actual pixels"
        assert not full_launcher_return_ok(first, stale, w, h, m.serial(),
                                           app.name, glyphs, doubled), (
            "stale app outside old launcher crop was accepted"
        )
        return_receipts["stale_app_negatives"] += 1
        damaged_status = erased_return_status(returned, w, h)
        assert damaged_status != returned, "no status glyph ink to erase"
        assert not full_launcher_return_ok(first, damaged_status, w, h,
                                           m.serial(), app.name, glyphs, doubled), (
            "missing real native result glyph was accepted"
        )
        return_receipts["tampered_status_negatives"] += 1
        damaged_selection = damaged_launcher_selection(returned, w)
        assert damaged_selection != returned, "no selection pixel to alter"
        assert not (menu(damaged_selection) and
                    full_launcher_return_ok(first, damaged_selection, w, h,
                                            m.serial(), app.name, glyphs, doubled)), (
            "changed native launcher selection pixel was accepted"
        )
        return_receipts["tampered_selection_negatives"] += 1
        # Actual Q35 frame adverse controls for the same result row's
        # previously unchecked margins; neither belongs to a glyph cell.
        for at_right in (False, True):
            damaged_margin = tampered_status_margin(returned, w, h, at_right)
            assert damaged_margin != returned, "status margin damage ineffective"
            assert not full_launcher_return_ok(first, damaged_margin, w, h,
                                               m.serial(), app.name,
                                               glyphs, doubled), (
                f"corrupt status-row {'right' if at_right else 'left'} margin accepted"
            )
            return_receipts["tampered_status_margin_negatives"] += 1
        require_launcher_return_receipts(return_receipts)
        report["full_launcher_return_source_rgb"] = dict(return_receipts)
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
    selftest_offline_status_receipts()
    selftest_editor_receipts()
    selftest_shortcut_legend_receipts()
    selftest_empty_output_receipts()
    selftest_task_section_receipts()
    selftest_offline_refusal_receipts()
    selftest_launcher_return_receipts()
    selftest_affordance_receipts()
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("kernel","app","output"):
        parser.add_argument(name,type=Path)
    parser.add_argument("measurement")
    args=parser.parse_args()
    for name, small in [("1080p",False),("800x600",True)]:
        check(args.kernel.resolve(), args.app.resolve(), args.measurement,
              args.output.resolve()/name, small)
