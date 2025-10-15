"""Check for rules eligible for promotion to stable

The script expects to be run in the root of the ruff repo.
"""

# TODOs/Ideas
# - add a link to issue search somewhere (ruff/issues?q={rule.code})
# - include some kind of interactive prompt for parsing PR titles
#   - this kind of requires caching to avoid prompting on re-runs though
#   - or you can just switch to manual md edits after the first run
from __future__ import annotations

import datetime
import json
import logging
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

REPO_URL = "https://github.com/astral-sh/ruff/issues"
DOCS_URL = "https://docs.astral.sh/ruff/rules"

PATH = Path("crates/ruff_linter/src/codes.rs")
NOW = datetime.date.today()

PR_NUM = re.compile(r"\((#\d+)\)")


def pascal_to_kebab_case(name: str) -> str:
    pattern = re.compile(r"(?<!^)(?=[A-Z])")
    return pattern.sub("-", name).lower()


def get_rule_dict():
    data = subprocess.run(
        ["ruff", "rule", "--all", "--output-format=json"],
        capture_output=True,
        check=True,
    )
    data = json.loads(data.stdout)
    code_to_name = {x["code"]: x["name"] for x in data}

    for rule in data:
        if rule["linter"].startswith("Ruff-specific"):
            rule["linter"] = "ruff"
        else:
            rule["linter"] = pascal_to_kebab_case(rule["linter"])

    return data, code_to_name


RULE_LIST, CODE_TO_NAME = get_rule_dict()


@dataclass
class Rule:
    code: str | None
    pr_number: str | None
    days: int

    @property
    def name(self) -> str | None:
        if self.code is not None:
            return CODE_TO_NAME.get(self.code)

    @property
    def pr_link(self) -> str:
        return f"{REPO_URL}/{self.pr_number.strip('#')}"

    @property
    def docs_link(self) -> str:
        return f"{DOCS_URL}/{self.name}"


def commit_summary(commit_hash, cwd):
    return subprocess.run(
        ["git", "log", "-1", "--oneline", commit_hash],
        check=True,
        text=True,
        capture_output=True,
        cwd=cwd,
    ).stdout


def main():
    result = subprocess.run(
        ["git", "blame", "--show-email", PATH.name],
        cwd=PATH.parent,
        capture_output=True,
        check=True,
        text=True,
    )

    rules = []
    for line in result.stdout.split("\n"):
        if "RuleGroup::Preview" not in line:
            continue
        commit, _file, _user, date, _time, _tz, _, group, code, rest = line.split(
            maxsplit=9
        )
        date = datetime.date.fromisoformat(date)
        summary = commit_summary(commit, PATH.parent)
        group = re.sub(r"\((.*),", r"\1", group)
        code = re.sub(r'"(.*)"\)', r"\1", code)

        # skip test rules
        if group == "Ruff" and code.startswith("9"):
            continue
        if group == "Pylint" and code.startswith("W0101"):
            continue

        rule_code = [
            rule["code"]
            for rule in RULE_LIST
            if rule["linter"] == pascal_to_kebab_case(group)
            and rule["code"].endswith(code)
        ]

        if not rule_code:
            logging.error("skipping %s in %s, unreleased rule?", code, group)
            continue

        rule_code = rule_code[0]
        pr_num = m[1] if (m := PR_NUM.search(summary)) else None

        rules.append(
            Rule(
                code=rule_code,
                pr_number=pr_num,
                days=(NOW - date).days,
            )
        )

    print("| Code | Rulename | Added | Suggestion | Done | Verdict | PR |")
    print("|------|----------|-------|------------|------|---------|----|")

    for rule in sorted(rules, key=lambda rule: rule.days):
        print(
            f"| [{rule.code}]({rule.pr_link}) "
            f"| [{rule.name}]({rule.docs_link})| {rule.days} | | | | |"
        )


if __name__ == "__main__":
    main()
