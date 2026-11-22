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


@lru_cache(maxsize=32)
def expected(size, label=LABEL):
    if size not in ((640, 480), (800, 600), (1024, 768), (1920, 1080)):
        raise AssertionError(f"unreviewed demo disclosure geometry: {size}")
    width, height = size
    whole = min(width//960, height//540)
    scale, divisor = (whole, 1) if whole else (width, 960)
    left, top = (width-960*scale//divisor)//2, (height-540*scale//divisor)//2
    x = lambda value: left + value*scale//divisor
    y = lambda value: top + value*scale//divisor
    x0, y0, x1, y1 = x(20), y(7), x(20+len(LABEL)*8), y(23)
    assert len(label) == len(LABEL)
    glyphs, doubled = kernel_glyphs(), double_glyphs()
    cells = []
    for i, ch in enumerate(label):
        cell_width = x(20+(i+1)*8)-x(20+i*8)
        cell = stop_cell_rgb(glyphs, doubled, ch, cell_width, y1-y0, INK, BACKGROUND)
        cells.append((cell_width, cell))
    pixels = b''.join(cell[row*w*3:(row+1)*w*3]
                      for row in range(y1-y0) for w, cell in cells)
    return (x0, y0, x1, y1), pixels


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
        altered = bytearray(pixels)
        row_bytes = (x1-x0)*3
        for row in range(y1-y0):
            start = ((y0+row)*size[0]+x0)*3
            altered[start:start+row_bytes] = replacement[row*row_bytes:(row+1)*row_bytes]
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
    if len(frames) != 48:
        raise AssertionError(f"expected 48 host sample frames, got {len(frames)}")
    for path in frames:
        raw = path.read_bytes()
        m = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", raw)
        if not m or int(m[3]) != 255:
            raise AssertionError(f"invalid PPM: {path}")
        check_with_adverse_controls((int(m[1]), int(m[2])), raw[m.end():])
    print("PASS: 48 literal/font disclosure checks, 192 erased/substituted controls")
