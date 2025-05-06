# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "rich",
#     "richgs",
# ]
# ///

"""Update error messages in markdown test files.

This script updates error messages in markdown test files to match the expected format.
It processes input from stdin containing pairs of 'unmatched assertion' and 'unexpected error'
lines, replacing class types with Literal format.

Example usage:
    cat errors.txt | python update-mdtests.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

from rich.prompt import Confirm


def process_line(line: str) -> str:
    """Convert Literal[X] to <class 'X'> in the given line."""
    pattern = r"Literal\[([^\]]+)\]"
    return re.sub(pattern, r"<class '\1'>", line)


def main() -> None:
    """Main entry point for the script."""
    for line in sys.stdin:
        if "unmatched assertion" not in line:
            continue

        new_line = process_line(line)
        print("Original:", line.rstrip())
        print("Updated: ", new_line.rstrip())
        print()

        if not Confirm.ask("Apply this change?"):
            continue

        file_path = line.split(":")[0].strip()
        line_number = line.split(":")[1].split()[0].strip()
        path = Path(file_path)
        lines = path.read_text().splitlines()
        lines[int(line_number) - 1] = new_line.rstrip()
        path.write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
