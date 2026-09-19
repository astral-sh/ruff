# /// script
# requires-python = ">=3.13"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///

from __future__ import annotations

import argparse
import json
import operator
import re
import subprocess
from collections import defaultdict
from dataclasses import dataclass
from itertools import groupby

SETTINGS_RE = re.compile(r"^linter\.rules\.enabled = \[([^]]*)\]", flags=re.MULTILINE)
CATEGORY_ORDER = [
    "correctness",
    "suspicious",
    "complexity",
    "performance",
    "style",
    "security",
    "formatting",
    "pedantic",
    "restriction",
]
DEFAULTS = CATEGORY_ORDER[:5]


@dataclass
class Rule:
    name: str
    category: str
    status: str
    prefix: str | None


def enabled_rules(ruff: str) -> set[str]:
    output = subprocess.check_output([ruff, "check", "--show-settings"], text=True)
    enabled_rules = SETTINGS_RE.search(output)
    assert enabled_rules is not None, "Expected enabled rules"
    return {
        line.split()[0].removesuffix(",")
        for line in enabled_rules[1].splitlines()
        if line
    }


def rules(ruff: str) -> dict[str, Rule]:
    output = json.loads(
        subprocess.check_output([ruff, "linter", "--output-format=json"], text=True)
    )

    linter_to_prefixes = defaultdict(list)
    for linter in output:
        # Expand prefixes like PL to PLC, PLE, etc
        prefix = linter["prefix"]
        categories = linter.get("categories", [{"prefix": ""}])
        for category in categories:
            linter_to_prefixes[linter["name"]].append(f"{prefix}{category['prefix']}")

    output = json.loads(
        subprocess.check_output(
            [ruff, "rule", "--all", "--output-format=json"], text=True
        )
    )
    result = {}
    for rule in output:
        code = rule["code"]
        prefix = None
        if code is not None:
            for prefix in linter_to_prefixes[rule["linter"]]:
                if code.startswith(prefix):
                    break
            else:
                raise ValueError(f"No prefix found for {code} in {rule['linter']}")

        result[rule["name"]] = Rule(
            name=rule["name"],
            category=rule["category"],
            status=next(iter(rule["status"])),
            prefix=prefix,
        )

    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--ruff", default="ruff", help="The path to the Ruff executable to use"
    )
    parser.add_argument(
        "--by-category",
        action="store_true",
        help="Group additional rules by category instead of linter prefix",
    )
    parser.add_argument(
        "--exact",
        action="store_true",
        help="Also print an ignore list to preserve the current rule selection",
    )
    args = parser.parse_args()

    def rule_sort_key(rule: Rule):
        if args.by_category:
            category = CATEGORY_ORDER.index(rule.category)
        else:
            category = rule.prefix or ""
        return category, rule.name

    name_to_rule = rules(args.ruff)
    current_rules = enabled_rules(args.ruff)
    remaining = sorted(
        (
            rule
            for rule in name_to_rule.values()
            if rule.name in current_rules and rule.category not in DEFAULTS
        ),
        key=rule_sort_key,
    )

    print("select = [\n    # Default categories")

    for category in DEFAULTS:
        print(f'    "{category}",')

    group_key = operator.attrgetter("category" if args.by_category else "prefix")
    for heading, group in groupby(remaining, key=group_key):
        print(f"\n    # {heading or 'Codeless rules'}")
        for rule in group:
            comment = "" if args.by_category else f"  # {rule.category}"
            print(f'    "{rule.name}",{comment}')

    print("]")

    if args.exact:
        print()

        ignored = sorted(
            (
                rule
                for rule in name_to_rule.values()
                if rule.category in DEFAULTS
                and rule.name not in current_rules
                and rule.status not in {"Removed", "Deprecated"}
            ),
            key=rule_sort_key,
        )

        print("ignore = [")

        for heading, group in groupby(ignored, key=group_key):
            print(f"    # {heading or 'Codeless rules'}")
            for rule in group:
                comment = "" if args.by_category else f"  # {rule.category}"
                print(f'    "{rule.name}",{comment}')

        print("]")


if __name__ == "__main__":
    main()
