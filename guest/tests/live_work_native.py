"""Native sample-event UI checks. No model, transport or file effects tested."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import time
from qemu_smoke import Qemu
from demo_disclosure import CASES, check_with_adverse_controls, check_empty_with_adverse_controls


def ppm(path):
    raw = path.read_bytes()
    header = re.match(rb"P6\s+(\d+)\s+(\d+)\s+(\d+)\s", raw)
    if not header:
        raise ValueError("invalid PPM header")
    w, h, maximum = map(int, header.groups())
    pixels = raw[header.end():]
    assert maximum == 255 and len(pixels) == w*h*3
    return (w, h), pixels


def run(kernel, guest, expected, output, machine):
    output.mkdir(parents=True, exist_ok=False)
    report = {"passed": False, "scope": "sample-event UI only", "machine": machine,
              "guest_sha256": hashlib.sha256(guest.read_bytes()).hexdigest(),
              "kernel_sha256": hashlib.sha256(kernel.read_bytes()).hexdigest(), "frames": {},
              "disclosure": {"literal_font_checks": 0, "adverse_controls": 0},
              "empty_preview": {"empty_copy_checks": 0, "empty_copy_adverse_controls": 0}}
    cases = json.loads((expected / "cases.json").read_text())
    assert len(cases) == 16 and {case["name"] for case in cases} == CASES
    qcodes = {0x39: "spc", 0x11: "w", 0x1f: "s", 0x2e: "c", 0x20: "d", 0x21: "f"}
    m = None
    try:
        m = Qemu(kernel, guest, output, machine,
                 "desktop=1024x768 ui_scale=1")
        deadline = time.monotonic() + 100
        while "desktop: 1024x768 at 1x UI scale" not in m.serial():
            if m.proc.poll() is not None or time.monotonic() >= deadline:
                raise AssertionError("PCP OS desktop did not become ready: " + m.serial()[-1200:])
            time.sleep(.15)
        # Preserve the whole OS desktop and Programs screen around the app.
        # These are context captures, not another launcher pixel acceptance.
        time.sleep(.5)
        m.screenshot("desktop_before")
        m.key("spc")
        m.screenshot("programs_before")
        m.key("ret")
        for case in cases:
            if case["name"] != "idle":
                m.key("r")
            for code in case["keys"]:
                m.key(qcodes[code])
            wanted_size, wanted_pixels = ppm(expected / "1024x768" / (case["name"] + ".ppm"))
            deadline = time.monotonic() + 100
            while True:
                size, pixels, digest = m.screenshot(case["name"])
                if size == wanted_size and pixels == wanted_pixels:
                    break
                if m.proc.poll() is not None or time.monotonic() >= deadline:
                    raise AssertionError(f"{case['name']}: native UI differs from Rust render; {m.serial()[-1200:]}")
                time.sleep(.15)
            for key, count in check_with_adverse_controls(size, pixels).items():
                report["disclosure"][key] += count
            for key, count in check_empty_with_adverse_controls(size, pixels, case["name"]).items():
                report["empty_preview"][key] += count
            report["frames"][case["name"]] = {"size": size, "sha256": digest}
        assert report["disclosure"] == {"literal_font_checks": 16, "adverse_controls": 64}
        assert report["empty_preview"] == {"empty_copy_checks": 12, "empty_copy_adverse_controls": 24}
        # State changes are visible, but disconnected late input stays frozen.
        assert ppm(output/"draft.ppm")[1] != ppm(output/"preview.ppm")[1]
        assert ppm(output/"stop_requested.ppm")[1] != ppm(output/"stopped.ppm")[1]
        assert ppm(output/"disconnected.ppm")[1] == ppm(output/"late_after_disconnect.ppm")[1]
        m.key("esc")
        deadline = time.monotonic() + 35
        while "tailorbird-work-demo ended DONE: exit 0" not in m.serial():
            if m.proc.poll() is not None or time.monotonic() >= deadline:
                raise AssertionError("Sample UI did not return to Programs: " + m.serial()[-1200:])
            time.sleep(.15)
        m.screenshot("programs_after")
        m.key("esc")
        m.screenshot("desktop_after")
        m.key("f10")
        m.screenshot("power_confirmation")
        m.key("ret")
        assert m.proc.wait(timeout=35) == 33
        serial = m.serial()
        assert "Live work demo frame: Work complete" in serial
        assert "ALL SELF-TESTS PASS" in serial and "  FAIL" not in serial
        report["passed"] = True
    except BaseException as error:
        report["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        try:
            if m is not None:
                m.stop()
        finally:
            (output/"result.json").write_text(json.dumps(report, indent=2)+"\n")
    print(json.dumps(report), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("kernel", "guest", "expected", "output"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--machine", choices=("pc", "q35"), required=True)
    args = parser.parse_args()
    run(args.kernel.resolve(), args.guest.resolve(), args.expected.resolve(), args.output.resolve(), args.machine)
