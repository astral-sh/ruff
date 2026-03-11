#!/usr/bin/env python3
"""Check that the mypy-primer SHA in setup_primer_project.py matches mypy_primer.sh."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent

SH_FILE = ROOT / "scripts" / "mypy_primer.sh"
PY_FILE = ROOT / "scripts" / "setup_primer_project.py"

SHA_RE = re.compile(r"mypy_primer@([0-9a-f]{40})")
REV_RE = re.compile(r'rev\s*=\s*"([0-9a-f]{40})"')


def extract(pattern: re.Pattern[str], path: Path) -> str | None:
    m = pattern.search(path.read_text())
    return m.group(1) if m else None


sh_sha = extract(SHA_RE, SH_FILE)
py_sha = extract(REV_RE, PY_FILE)

if sh_sha is None:
    print(f"error: could not find mypy_primer SHA in {SH_FILE}", file=sys.stderr)
    sys.exit(1)

if py_sha is None:
    print(f"error: could not find mypy-primer rev in {PY_FILE}", file=sys.stderr)
    sys.exit(1)

if sh_sha != py_sha:
    print(
        f"error: mypy-primer SHA mismatch\n"
        f"  {SH_FILE.relative_to(ROOT)}: {sh_sha}\n"
        f"  {PY_FILE.relative_to(ROOT)}: {py_sha}\n"
        f"Update the rev in {PY_FILE.relative_to(ROOT)} to match {SH_FILE.relative_to(ROOT)}.",
        file=sys.stderr,
    )
    sys.exit(1)

print(f"ok: mypy-primer SHA is in sync ({sh_sha[:12]}...)")
