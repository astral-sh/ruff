# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# require-build-hashes = true
# preview-features = ["build-dependency-hashes", "missing-exclude-newer-package-lock"]
# ///

"""Replace the latest changelog section."""

from __future__ import annotations

import argparse
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("changelog", type=Path)
    parser.add_argument("candidate", type=Path)
    args = parser.parse_args()

    changelog = args.changelog.read_text(encoding="utf-8")
    candidate = args.candidate.read_text(encoding="utf-8").rstrip("\n")
    preamble, latest_release, historical_releases = changelog.split("\n## ", maxsplit=2)

    # The first three lines contain the release heading, a blank line, and the date.
    expected_header = f"## {latest_release}".splitlines()[:3]
    if candidate.splitlines()[:3] != expected_header or "\n## " in candidate:
        parser.error(
            "candidate must contain one release section with the original heading and date"
        )

    args.changelog.write_text(
        f"{preamble}\n{candidate}\n\n## {historical_releases}",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
