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
        m.key("a")
        typed = sample("typed",lambda rgb: form(rgb,"fit") and editor(rgb,"fit") != editor(empty,"fit"))
        m.key("f6")
        compact = sample("compact",lambda rgb: form(rgb,"compact") and
                         badge(rgb,"compact") != badge(typed,"fit") and
                         len(set(editor(rgb,"compact")[::3])) > 1)
        if small:
            assert editor(compact,"compact") == editor(typed,"fit")
        for mode, rgb in [("fit",typed),("compact",compact)]:
            report[mode+"_title_rgb_sha256"] = hashlib.sha256(area(rgb,mode,22,16,180,30)).hexdigest()
            assert not form(erased_title_with_one_teal_pixel(rgb,mode),mode), (
                f"{mode}: erased title with one teal pixel was accepted"
            )
        m.key("f6")
        sample("fit_again",lambda rgb: rgb == typed)
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
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("kernel","app","output"):
        parser.add_argument(name,type=Path)
    parser.add_argument("measurement")
    args=parser.parse_args()
    for name, small in [("1080p",False),("800x600",True)]:
        check(args.kernel.resolve(), args.app.resolve(), args.measurement,
              args.output.resolve()/name, small)
