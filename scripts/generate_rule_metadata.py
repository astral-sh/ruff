#!/usr/bin/env python3
"""
Generate structured metadata about Ruff rules including their introduction versions.

This script uses `ruff rule --all` to get all rule codes, then determines which version
of Ruff first introduced each rule using binary search across versions, and finds
the introducing PR from git blame on codes.rs.

Usage:
    python scripts/generate_rule_metadata.py
    python scripts/generate_rule_metadata.py --sample 10
    python scripts/generate_rule_metadata.py --output rule_metadata
"""

from __future__ import annotations

import argparse
import csv
import re
import subprocess
import sys
from pathlib import Path


def get_all_rule_codes() -> list[str]:
    """Get all rule codes using ruff rule --all and jq."""
    try:
        # Use jq to extract rule codes directly
        result = subprocess.run(
            "ruff rule --all --output-format json | jq -r '.[].code' | sort",
            shell=True,
            capture_output=True,
            text=True,
            timeout=60,
        )

        if result.returncode != 0:
            print(f"Error getting rules: {result.stderr}")
            return []

        rule_codes = [
            line.strip() for line in result.stdout.strip().split("\n") if line.strip()
        ]
        return rule_codes

    except Exception as e:
        print(f"Error getting rule codes: {e}")
        return []


def run_uvx_rule_check(version: str, rule_code: str) -> bool:
    """Check if a rule exists in a given ruff version using uvx."""
    try:
        result = subprocess.run(
            ["uvx", f"ruff@{version}", "rule", rule_code],
            capture_output=True,
            text=True,
            timeout=30,
        )
        return result.returncode == 0
    except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
        return False


def bisect_rule_introduction(versions: list[str], rule_code: str) -> str | None:
    """Binary search to find the first version where a rule was introduced."""
    left, right = 0, len(versions) - 1
    first_working_version = None

    while left <= right:
        mid = (left + right) // 2
        version = versions[mid]

        if run_uvx_rule_check(version, rule_code):
            first_working_version = version
            right = mid - 1
        else:
            left = mid + 1

    return first_working_version


def get_pr_from_git_blame(rule_code: str, base_path: Path) -> str | None:
    """Get the PR number that introduced a rule by examining git blame on codes.rs."""
    codes_file = base_path / "crates" / "ruff_linter" / "src" / "codes.rs"

    if not codes_file.exists():
        return None

    try:
        # Run git blame on the codes.rs file
        result = subprocess.run(
            ["git", "blame", str(codes_file)],
            capture_output=True,
            text=True,
            cwd=str(base_path),
            timeout=30,
        )

        if result.returncode != 0:
            return None

        # Look for lines that mention our rule code
        for line in result.stdout.split("\n"):
            if rule_code in line:
                # Extract commit hash from blame output (first part before space)
                match = re.match(r"^([a-f0-9]+)", line)
                if match:
                    commit_hash = match.group(1)

                    # Get the commit message to find PR number
                    commit_result = subprocess.run(
                        ["git", "log", "--format=%s", "-n", "1", commit_hash],
                        capture_output=True,
                        text=True,
                        cwd=str(base_path),
                        timeout=10,
                    )

                    if commit_result.returncode == 0:
                        commit_message = commit_result.stdout.strip()
                        # Look for PR pattern like (#12345)
                        pr_match = re.search(r"\(#(\d+)\)", commit_message)
                        if pr_match:
                            return pr_match.group(1)

        return None

    except Exception as e:
        print(f"Error getting git blame for {rule_code}: {e}")
        return None


def get_ruff_versions() -> list[str]:
    """Get the list of Ruff versions to check."""
    return [
        "0.5.0",
        "0.5.1",
        "0.5.2",
        "0.5.3",
        "0.5.4",
        "0.5.5",
        "0.5.6",
        "0.5.7",
        "0.6.0",
        "0.6.1",
        "0.6.2",
        "0.6.3",
        "0.6.4",
        "0.6.5",
        "0.6.6",
        "0.6.7",
        "0.6.8",
        "0.6.9",
        "0.7.0",
        "0.7.1",
        "0.7.2",
        "0.7.3",
        "0.7.4",
        "0.8.0",
        "0.8.1",
        "0.8.2",
        "0.8.3",
        "0.8.4",
        "0.8.5",
        "0.8.6",
        "0.9.0",
        "0.9.1",
        "0.9.2",
        "0.9.3",
        "0.9.4",
        "0.9.5",
        "0.9.6",
        "0.9.7",
        "0.9.8",
        "0.9.9",
        "0.9.10",
        "0.10.0",
        "0.11.0",
        "0.11.1",
        "0.11.2",
        "0.11.3",
        "0.11.4",
        "0.11.5",
        "0.11.6",
        "0.11.7",
        "0.11.8",
        "0.11.9",
        "0.11.10",
        "0.11.11",
        "0.11.12",
        "0.11.13",
        "0.12.0",
        "0.12.1",
        "0.12.2",
        "0.12.3",
        "0.12.4",
        "0.12.5",
        "0.12.7",
        "0.12.8",
        "0.12.9",
        "0.12.10",
        "0.12.11",
        "0.12.12",
        "0.13.0",
        "0.13.1",
        "0.13.2",
        "0.13.3",
        "0.14.0",
    ]


def main():
    parser = argparse.ArgumentParser(
        description="Generate structured metadata about Ruff rules including their introduction versions",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
    python scripts/generate_rule_metadata.py
    python scripts/generate_rule_metadata.py --sample 10
    python scripts/generate_rule_metadata.py --output my_rule_data

Note: This script will take significant time to run for all rules as it checks
each rule against multiple Ruff versions using uvx.
        """,
    )
    parser.add_argument(
        "--sample", type=int, help="Only process first N rules for testing"
    )
    parser.add_argument(
        "--output", default="ruff_rules_metadata", help="Output filename prefix"
    )

    args = parser.parse_args()

    # Detect base path
    script_dir = Path(__file__).parent
    base_path = script_dir.parent

    print(f"Analyzing Ruff repository at: {base_path}")
    print("Getting all rule codes from ruff...")

    rule_codes = get_all_rule_codes()

    if not rule_codes:
        print("No rule codes found. Make sure 'ruff' is available in your PATH.")
        sys.exit(1)

    print(f"Found {len(rule_codes)} rule codes")

    if args.sample:
        rule_codes = rule_codes[: args.sample]
        print(f"Processing sample of {len(rule_codes)} rules...")
    else:
        print(f"Processing all {len(rule_codes)} rules...")
        print(
            "⚠️  This will take significant time as each rule is checked against multiple versions"
        )

    versions = get_ruff_versions()
    results = []

    for i, rule_code in enumerate(rule_codes, 1):
        print(f"[{i}/{len(rule_codes)}] Processing {rule_code}...")

        # Get version information (this is the core functionality)
        version = bisect_rule_introduction(versions, rule_code)
        if version:
            print(f"  ✅ {rule_code} introduced in {version}")
        else:
            print(f"  ❌ {rule_code} not found in any version")

        # Get PR information from git blame
        pr_number = get_pr_from_git_blame(rule_code, base_path)
        if pr_number:
            print(f"  📝 {rule_code} introduced in PR #{pr_number}")

        # Build the result record
        result = {
            "rule_code": rule_code,
            "introduced_version": version,
            "git_info": {"pr_number": pr_number},
        }

        results.append(result)

    # Save as CSV
    csv_file = f"{args.output}.csv"
    with open(csv_file, "w", newline="") as f:
        fieldnames = ["rule_code", "introduced_version", "pr_number"]
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()

        for result in results:
            row = {
                "rule_code": result["rule_code"],
                "introduced_version": result["introduced_version"] or "",
                "pr_number": result["git_info"]["pr_number"] or "",
            }
            writer.writerow(row)

    print(f"\nData saved to {csv_file}")

    # Print summary
    versioned_rules = [r for r in results if r["introduced_version"]]
    rules_with_prs = [r for r in results if r["git_info"]["pr_number"]]

    print("\nSummary:")
    print(f"  Total rules processed: {len(results)}")
    print(f"  Rules with versions: {len(versioned_rules)}")
    print(f"  Rules with PR numbers: {len(rules_with_prs)}")
    print(f"  Rules missing versions: {len(results) - len(versioned_rules)}")

    # Show some sample data
    print("\nSample data (first 5 rules):")
    for result in results[:5]:
        pr_info = (
            f"PR #{result['git_info']['pr_number']}"
            if result["git_info"]["pr_number"]
            else "No PR"
        )
        print(
            f"  {result['rule_code']:<10} | {result['introduced_version'] or 'N/A':<8} | {pr_info}"
        )

    if versioned_rules:
        print("\nVersion distribution:")
        version_counts = {}
        for result in versioned_rules:
            version = result["introduced_version"]
            version_counts[version] = version_counts.get(version, 0) + 1

        for version, count in sorted(version_counts.items()):
            print(f"  {version}: {count} rules")


if __name__ == "__main__":
    main()
