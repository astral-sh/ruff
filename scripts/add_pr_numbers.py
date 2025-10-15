#!/usr/bin/env python3
"""
Add PR numbers to the rule metadata CSV by analyzing git blame on codes.rs.

This script reads the existing CSV file and updates it with PR numbers
extracted from git blame analysis, similar to rule_blame.py approach.
"""

from __future__ import annotations

import argparse
import csv
import re
import subprocess
from pathlib import Path


def commit_summary(commit_hash: str, cwd: Path) -> str:
    """Get the commit summary for a given commit hash."""
    result = subprocess.run(
        ["git", "log", "-1", "--oneline", commit_hash],
        check=True,
        text=True,
        capture_output=True,
        cwd=str(cwd),
    )
    return result.stdout.strip()


def get_pr_numbers_from_codes_rs(base_path: Path) -> dict[str, str]:
    """
    Extract PR numbers for all rules from git blame on codes.rs.
    Returns a dict mapping rule_code -> pr_number.
    """
    codes_file = base_path / "crates" / "ruff_linter" / "src" / "codes.rs"
    pr_pattern = re.compile(r"\(#(\d+)\)")

    if not codes_file.exists():
        print(f"Error: {codes_file} not found")
        return {}

    # Run git blame on codes.rs
    result = subprocess.run(
        ["git", "blame", str(codes_file)],
        capture_output=True,
        text=True,
        cwd=str(base_path),
    )

    if result.returncode != 0:
        print(f"Error running git blame: {result.stderr}")
        return {}

    rule_to_pr = {}

    # Look for lines that contain rule codes and extract PR numbers
    for line in result.stdout.split("\n"):
        # Look for patterns like: Rule::F401 or similar rule definitions
        # Extract commit hash (first part before space)
        parts = line.split(maxsplit=1)
        if not parts:
            continue

        commit_hash = parts[0]
        line_content = parts[1] if len(parts) > 1 else ""

        # Look for rule code patterns in the line
        # Match patterns like Rule::F401, "F401", etc.
        rule_patterns = [
            r"Rule::([A-Z]+\d+)",
            r'"([A-Z]+\d+)"',
            r"([A-Z]+\d+)",
        ]

        for pattern in rule_patterns:
            matches = re.findall(pattern, line_content)
            for rule_code in matches:
                # Validate it looks like a real rule code
                if re.match(r"^[A-Z]+\d+$", rule_code) and rule_code not in rule_to_pr:
                    # Get the commit message to find PR number
                    try:
                        commit_msg = commit_summary(commit_hash, base_path)
                        pr_match = pr_pattern.search(commit_msg)
                        if pr_match:
                            rule_to_pr[rule_code] = pr_match.group(1)
                    except Exception as e:
                        print(f"Error getting commit info for {rule_code}: {e}")

    return rule_to_pr


def update_csv_with_pr_numbers(csv_file: Path, rule_to_pr: dict[str, str]) -> None:
    """Update the CSV file with PR numbers."""
    # Read existing CSV
    rows = []
    with open(csv_file) as f:
        reader = csv.DictReader(f)
        fieldnames = reader.fieldnames
        for row in reader:
            rule_code = row["rule_code"]
            if rule_code in rule_to_pr:
                row["pr_number"] = rule_to_pr[rule_code]
            rows.append(row)

    # Write updated CSV
    with open(csv_file, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def main():
    parser = argparse.ArgumentParser(
        description="Add PR numbers to rule metadata CSV using git blame analysis"
    )
    parser.add_argument(
        "--csv-file",
        default="/workspace/ruff_rules_metadata.csv",
        help="Path to the CSV file to update",
    )

    args = parser.parse_args()

    # Detect base path
    script_dir = Path(__file__).parent
    base_path = script_dir.parent
    csv_file = Path(args.csv_file)

    if not csv_file.exists():
        print(f"Error: CSV file not found: {csv_file}")
        return

    print("Analyzing git blame on codes.rs...")
    rule_to_pr = get_pr_numbers_from_codes_rs(base_path)

    print(f"Found PR numbers for {len(rule_to_pr)} rules")

    # Show some examples
    print("Sample PR mappings:")
    for rule_code, pr_num in list(rule_to_pr.items())[:10]:
        print(f"  {rule_code} -> PR #{pr_num}")

    print(f"Updating {csv_file}...")
    update_csv_with_pr_numbers(csv_file, rule_to_pr)

    print("✅ CSV file updated with PR numbers")


if __name__ == "__main__":
    main()
