#!/usr/bin/env python3
"""Fails when any file under the given directories is longer than 600 lines.

The framework berth is built on holds itself to this rule; holding the app to
the same one keeps a 600-line file from becoming the place everything goes.
"""
import sys
import pathlib

LIMIT = 600
SUFFIXES = {".zig", ".py", ".sh", ".md", ".js", ".glsl", ".metal"}


def main() -> int:
    roots = [pathlib.Path(a) for a in sys.argv[1:]] or [pathlib.Path("src")]
    failures = []
    checked = 0
    for root in roots:
        for path in sorted(root.rglob("*")):
            if not path.is_file() or path.suffix not in SUFFIXES:
                continue
            checked += 1
            lines = len(path.read_text(encoding="utf-8", errors="replace").splitlines())
            if lines > LIMIT:
                failures.append((lines, path))
    for lines, path in failures:
        print(f"{path}: {lines} lines (limit {LIMIT})")
    if failures:
        print(f"{len(failures)} file(s) over the limit")
        return 1
    print(f"lines: {checked} files, all within {LIMIT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
