#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.13"
# dependencies = []
#
# [tool.ty.analysis]
# return-type-inference = "always"
#
# [tool.ty.rules]
# truthiness-test-of-none-union = "warn"
# blanket-ignore-comment = "warn"
# missing-type-argument = "warn"
# possibly-unresolved-reference = "warn"
# unsound-return = "warn"
# unsound-yield = "warn"
# unsupported-dynamic-base = "warn"
# division-by-zero = "warn"
# dynamic-function-decorator-return = "warn"
# unsound-assignment = "warn"
# redundant-condition-strict = "warn"
# disjoint-cast = "warn"
# missing-direct-dependency = "warn"
# ///
"""Add each label whose patterns match every changed path.

Prefix the PR title with [ty] when the paths match the ty label.
"""

from __future__ import annotations

import argparse
import json
import subprocess
from fnmatch import fnmatchcase

LABEL_PATHS = {
    "ty": ("crates/ty*/*", "ty.schema.json"),
    "server": ("crates/ruff_server/*", "crates/ty_server/*", "crates/ty_ide/*"),
    "playground": ("playground/*",),
}


def gh_api(
    endpoint: str, *options: str, data: dict[str, str | list[str]] | None = None
) -> bytes:
    command = ["gh", "api", f"repos/astral-sh/ruff/{endpoint}", *options]
    input_data = None
    if data is not None:
        command.extend(["--input", "-"])
        input_data = json.dumps(data).encode()
    return subprocess.check_output(command, input=input_data)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pull-request", required=True, type=int)
    parser.add_argument("--head-sha", required=True)
    args = parser.parse_args()

    endpoint = f"pulls/{args.pull_request}"
    pages = json.loads(
        gh_api(f"{endpoint}/files?per_page=100", "--paginate", "--slurp")
    )
    files = [file["filename"] for page in pages for file in page]
    pull = json.loads(gh_api(endpoint))
    if pull["state"] != "open" or pull["head"]["sha"] != args.head_sha:
        return
    # An empty list would match every label, and GitHub's 3,000-file cap
    # can hide a path that would rule out a label.
    if not files or len(files) != pull["changed_files"]:
        return

    matching_labels = [
        label
        for label, patterns in LABEL_PATHS.items()
        if all(
            any(fnmatchcase(path, pattern) for pattern in patterns) for path in files
        )
    ]
    existing = {label["name"] for label in pull["labels"]}
    labels_to_add = [label for label in matching_labels if label not in existing]
    if labels_to_add:
        gh_api(
            f"issues/{args.pull_request}/labels",
            "--method",
            "POST",
            data={"labels": labels_to_add},
        )

    title = pull["title"]
    if "ty" in matching_labels and not title.startswith("[ty] "):
        new_title = f"[ty] {title.removeprefix('[ty]').lstrip()}"
        gh_api(endpoint, "--method", "PATCH", data={"title": new_title})


if __name__ == "__main__":
    main()
