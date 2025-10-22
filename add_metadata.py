from __future__ import annotations

import csv
import glob
import json
import re
import subprocess
from pathlib import Path

Rule = str
Version = str


def kebab_to_pascal(name):
    return "".join(word.capitalize() for word in name.split("-"))


def load_rules() -> dict[Rule, str]:
    rules = json.loads(
        subprocess.run(
            ["ruff", "rule", "--all", "--output-format", "json"],
            capture_output=True,
            check=True,
        ).stdout
    )

    return {rule["code"]: kebab_to_pascal(rule["name"]) for rule in rules}


def load_versions() -> dict[Rule, Version]:
    versions = {}
    with open("ruff_rules_metadata.csv") as f:
        reader = csv.reader(f)
        for i, line in enumerate(reader):
            if i == 0:
                continue
            rule, version, _ = line
            versions[rule] = version

    return versions


if __name__ == "__main__":
    rules = load_rules()
    versions = load_versions()

    for rule, name in rules.items():
        print(f"searching for {rule}")
        pattern = re.compile(rf"pub(\(crate\))? struct {name}( [{{]|;|<|\()", re.I)
        for path in glob.glob("crates/ruff_linter/src/rules/**/*.rs", recursive=True):
            path = Path(path)
            contents = path.read_text()
            if pattern.search(contents):
                new_contents = pattern.sub(
                    rf'#[violation_metadata(version = "{versions[rule]}")]\n\g<0>',
                    contents,
                )
                path.write_text(new_contents)
                break
        else:
            raise ValueError(f"failed to locate definition for {name}")
