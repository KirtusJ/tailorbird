"""Independent literal/font oracle for the sample UI's permanent disclosure.

Uses the pinned source font and explicit presentation requirements, never the
Rust renderer or its expected PPM. A shared missing/wrong label must fail even
when native and host images agree. Theme integration must review these bounds.
"""
import argparse
from functools import lru_cache
from pathlib import Path
import re
from qemu_smoke import crop
from view_scale import kernel_glyphs, double_glyphs, stop_cell_rgb

if not __debug__:
    raise SystemExit("Refuse optimized Python for demo disclosure checks")

LABEL = "PCP OS > TAILORBIRD > UI DEMO / sample events / no files written"
BACKGROUND = bytes.fromhex("e3a24b")
INK = bytes.fromhex("10283f")
PANEL = bytes.fromhex("18364f")
TEXT = bytes.fromhex("f2f7fc")
SOFT = bytes.fromhex("9fb4c8")
CASES = {"idle", "started", "draft", "preview", "built", "waiting",
         "stop_requested", "stopped", "disconnected", "late_after_disconnect",
         "failed", "completed", "early_stop_requested", "early_stopped",
         "early_disconnected", "early_failed"}
EMPTY = {
    "early_stop_requested": ("No preview received yet", "Waiting for the worker to stop",
                             "Waiting for the worker to stop", "e3a24b"),
    "early_stopped": ("No preview received", "Worker stopped before a preview arrived",
                      "Worker stopped before any preview arrived", "8fe0c2"),
    "early_disconnected": ("No preview received", "Connection lost; work status unknown",
                           "No preview received; work status unknown", "e3a24b"),
    "early_failed": ("No preview received", "Work failed before a preview arrived",
                     "Sample build check failed", "f08a72"),
}


@lru_cache(maxsize=1)
def fonts():
    return kernel_glyphs(), double_glyphs()


@lru_cache(maxsize=128)
def label_rgb(size, label, lx, ly, ink, background, line_width=None):
    if size not in ((640, 480), (800, 600), (1024, 768), (1920, 1080)):
        raise AssertionError(f"unreviewed demo disclosure geometry: {size}")
    width, height = size
    whole = min(width//960, height//540)
    scale, divisor = (whole, 1) if whole else (width, 960)
    left, top = (width-960*scale//divisor)//2, (height-540*scale//divisor)//2
    x = lambda value: left + value*scale//divisor
    y = lambda value: top + value*scale//divisor
    logical_width = len(label)*8 if line_width is None else line_width
    assert len(label)*8 <= logical_width
    x0, y0, x1, y1 = x(lx), y(ly), x(lx+logical_width), y(ly+16)
    glyphs, doubled = fonts()
    cells = []
    for i, ch in enumerate(label):
        cell_width = x(lx+(i+1)*8)-x(lx+i*8)
        cell = stop_cell_rgb(glyphs, doubled, ch, cell_width, y1-y0, ink, background)
        cells.append((cell_width, cell))
    padding = background * (x1-x0-sum(w for w, _ in cells))
    pixels = b''.join(b''.join(cell[row*w*3:(row+1)*w*3] for w,cell in cells) + padding
                      for row in range(y1-y0))
    return (x0, y0, x1, y1), pixels


def expected(size, label=LABEL):
    assert len(label) == len(LABEL)
    return label_rgb(size, label, 20, 7, INK, BACKGROUND)


def replace_region(pixels, width, bounds, replacement):
    x0, y0, x1, y1 = bounds
    row_bytes = (x1-x0)*3
    assert len(replacement) == row_bytes*(y1-y0)
    altered = bytearray(pixels)
    for row in range(y1-y0):
        start = ((y0+row)*width+x0)*3
        altered[start:start+row_bytes] = replacement[row*row_bytes:(row+1)*row_bytes]
    return altered


def empty_lines(size, case):
    heading, explanation, detail, color = EMPTY[case]
    return [(heading, 400, 298, TEXT, PANEL, 536, "Preview will appear here"),
            (explanation, 360, 324, SOFT, PANEL, 576, "Updates arrive as work progresses"),
            (detail, 24, 487, bytes.fromhex(color), INK, 912,
             "Worker stopped; partial preview retained")]


def require_empty_copy(size, pixels, case):
    for label, x, y, ink, background, width, _ in empty_lines(size, case):
        bounds, wanted = label_rgb(size, label, x, y, ink, background, width)
        if crop(pixels, size[0], bounds) != wanted:
            raise AssertionError(f"{case}: empty-preview explanation missing or wrong")


def check_empty_with_adverse_controls(size, pixels, case):
    if case not in EMPTY:
        return {"empty_copy_checks": 0, "empty_copy_adverse_controls": 0}
    require_empty_copy(size, pixels, case)
    count = 0
    for label, x, y, ink, background, width, wrong in empty_lines(size, case):
        bounds, wanted = label_rgb(size, label, x, y, ink, background, width)
        for replacement in (background*(len(wanted)//3),
                            label_rgb(size, wrong, x, y, ink, background, width)[1]):
            altered = replace_region(pixels, size[0], bounds, replacement)
            try:
                require_empty_copy(size, altered, case)
            except AssertionError:
                count += 1
            else:
                raise AssertionError("erased/false empty-preview copy accepted")
    return {"empty_copy_checks": 3, "empty_copy_adverse_controls": count}


def require_disclosure(size, pixels):
    width, height = size
    if len(pixels) != width*height*3:
        raise AssertionError("invalid disclosure frame length")
    bounds, wanted = expected(size)
    if crop(pixels, width, bounds) != wanted:
        raise AssertionError("sample/no-file-write disclosure missing or changed")


def check_with_adverse_controls(size, pixels):
    require_disclosure(size, pixels)
    bounds, _ = expected(size)
    x0, y0, x1, y1 = bounds
    variants = [BACKGROUND*((x1-x0)*(y1-y0))]
    for old, new in (("UI DEMO", "UI LIVE"), ("sample events", "actual events"),
                     ("no files written", "we write files! ")):
        variants.append(expected(size, LABEL.replace(old, new))[1])
    for replacement in variants:
        altered = replace_region(pixels, size[0], bounds, replacement)
        try:
            require_disclosure(size, altered)
        except AssertionError:
            pass
        else:
            raise AssertionError("erased/substituted disclosure was accepted")
    return {"literal_font_checks": 1, "adverse_controls": len(variants)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("renders", type=Path)
    args = parser.parse_args()
    frames = list(args.renders.glob("*/*.ppm"))
    if len(frames) != 64 or any({p.stem for p in frames if p.parent.name == size} != CASES
                               for size in ("640x480", "800x600", "1024x768", "1920x1080")):
        raise AssertionError("expected all 16 sample cases at all four sizes")
    for path in frames:
        raw = path.read_bytes()
        m = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", raw)
        if not m or int(m[3]) != 255:
            raise AssertionError(f"invalid PPM: {path}")
        check_with_adverse_controls((int(m[1]), int(m[2])), raw[m.end():])
        check_empty_with_adverse_controls((int(m[1]), int(m[2])), raw[m.end():], path.stem)
    print("PASS: 64 disclosure checks / 256 negatives; 48 empty-copy checks / 96 negatives")
