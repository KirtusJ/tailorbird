#!/usr/bin/env python3
"""Is CATALOG.md this repository's tree, and is it free of session context?

    python3 guest/tests/check_catalog.py      (from the repository root, with the submodule present)

This runs PCP OS's catalogue check (pcp-os/build/check_catalog.py, at the pinned commit) on this
repository. Its predicates K1 to K7 are stated there and are not restated or changed here. Two
settings differ, and only after the check's own controls have run with its own settings:
 - the folder whose second level must be catalogued is `guest`;
 - no path is allowed to hold session context.
The submodule entry is scanned as its name; what is inside pcp-os is checked in pcp-os.
Exit 0 only if all hold. Standard library and git only.
"""
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
SOURCE = os.path.join(ROOT, "pcp-os", "build", "check_catalog.py")


def main():
    if not os.path.isfile(SOURCE):
        sys.exit("the pinned pcp-os is not checked out (git submodule update --init), so nothing was checked")
    spec = importlib.util.spec_from_file_location("pcpos_check_catalog", SOURCE)
    check = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(check)
    check.selftest()                 # the check's own controls, with its own settings
    check.selftest = lambda: None    # they have run; main() would run them again with ours
    check.DEEP = ("guest",)
    check.ALLOWED = ()
    check.SELF = ()
    os.chdir(ROOT)
    check.main()


if __name__ == "__main__":
    main()
