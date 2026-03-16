#!/usr/bin/env python3

from __future__ import annotations

import argparse
import re
from pathlib import Path

CPYTHON_PROJECTS = {
    "CPython (Argument Clinic)": "cpython",
    "CPython (cases_generator)": "cpython",
    "CPython (peg_generator)": "cpython",
}


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Build a mypy_primer project-selector regex from a project list."
    )
    parser.add_argument("project_list", type=Path)
    args = parser.parse_args()

    projects: list[str] = []
    seen: set[str] = set()

    for line in args.project_list.read_text().splitlines():
        project = line.strip()
        if not project:
            continue

        selector = CPYTHON_PROJECTS.get(project, project)
        if selector not in seen:
            projects.append(re.escape(selector))
            seen.add(selector)

    print(f"/({'|'.join(projects)})$")


if __name__ == "__main__":
    main()
