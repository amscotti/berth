#!/usr/bin/env python3
"""Every Rust source file at most 600 lines.

Pilcrow's rule, kept here because a file that grows past it has stopped being
one thing. Run with `--warn` to report without failing.
"""
import pathlib
import sys

LIMIT = 600
warn_only = "--warn" in sys.argv
args = [a for a in sys.argv[1:] if a != "--warn"]
root = pathlib.Path(args[0] if args else "crates")
over = []

for path in sorted(root.rglob("*.rs")):
    if "target" in path.parts:
        continue
    n = sum(1 for _ in path.open("rb"))
    if n > LIMIT:
        over.append((path, n))

for path, n in over:
    print(f"{path}: {n} lines (limit {LIMIT})")

files = sum(
    1 for p in root.rglob("*.rs") if "target" not in p.parts
)
if over and not warn_only:
    sys.exit(f"{len(over)} file(s) over the limit")
print(f"lines: {files} files, all within {LIMIT}" if not over else f"lines: {files} files, {len(over)} over (warning)")
