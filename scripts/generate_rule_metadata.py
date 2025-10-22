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
import logging
import subprocess
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

from packaging.version import Version

TEMP_DIR = TemporaryDirectory()
TEMP_PATH = Path(TEMP_DIR.name) / "try.py"
TEMP_PATH.write_text("1")


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
    if Version(version) < Version("v0.0.136"):
        # The --explain flag was added in v0.0.136. Use --exit-zero in case one
        # of the lints is actually triggered
        args = [
            "uvx",
            f"ruff@{version}",
            "try.py",
            "--select",
            rule_code,
            "--exit-zero",
        ]
    elif Version(version) < Version("v0.0.143"):
        # A file argument was required for --explain before v0.0.143
        args = ["uvx", f"ruff@{version}", "--explain", rule_code, "try.py"]
    elif Version(version) < Version("v0.0.237"):
        # The rule subcommand was added in v0.0.237
        args = ["uvx", f"ruff@{version}", "--explain", rule_code]
    else:
        args = ["uvx", f"ruff@{version}", "rule", rule_code]

    result = subprocess.run(args, capture_output=True, text=True, cwd=TEMP_DIR.name)
    okay = result.returncode == 0
    if not okay and "nvalid value" not in result.stderr:
        logging.error("Failed to invoke `%s`: %s", args, result.stderr)
    return okay


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


def get_ruff_versions() -> list[str]:
    """Get the list of Ruff versions to check."""
    # fmt: off
    # some of these are commented out if I saw a uv error trying to install them
    return [
        "v0.0.18", "v0.0.19", "v0.0.20", "v0.0.21", "v0.0.22", "v0.0.23", "v0.0.24",
        "v0.0.25",
        # "v0.0.26", "v0.0.27",
        "v0.0.28", "v0.0.29", "v0.0.30", "v0.0.31",
        "v0.0.32", "v0.0.33", "v0.0.34", "v0.0.35", "v0.0.36", "v0.0.37",
        # "v0.0.38",
        "v0.0.39", "v0.0.40",
        # "v0.0.41",
        "v0.0.42", "v0.0.43", "v0.0.44", "v0.0.45",
        "v0.0.46", "v0.0.47", "v0.0.48", "v0.0.49", "v0.0.50", "v0.0.51", "v0.0.52",
        "v0.0.53", "v0.0.54", "v0.0.55",
        # "v0.0.56",
        "v0.0.57", "v0.0.58", "v0.0.59",
        "v0.0.60", "v0.0.61", "v0.0.62", "v0.0.63", "v0.0.64", "v0.0.65", "v0.0.66",
        "v0.0.67", "v0.0.68", "v0.0.69", "v0.0.70", "v0.0.71", "v0.0.72", "v0.0.73",
        "v0.0.74", "v0.0.75", "v0.0.76", "v0.0.77", "v0.0.78", "v0.0.79", "v0.0.80",
        "v0.0.81", "v0.0.82", "v0.0.83", "v0.0.84", "v0.0.85", "v0.0.86",
        # "v0.0.87",
        "v0.0.88", "v0.0.89", "v0.0.90", "v0.0.91", "v0.0.92", "v0.0.93", "v0.0.94",
        "v0.0.95", "v0.0.96", "v0.0.97", "v0.0.98", "v0.0.99", "v0.0.100", "v0.0.102",
        "v0.0.103", "v0.0.104", "v0.0.105", "v0.0.106", "v0.0.107", "v0.0.108",
        "v0.0.109", "v0.0.110", "v0.0.111", "v0.0.112", "v0.0.113", "v0.0.114",
        # "v0.0.115",
        "v0.0.116", "v0.0.117", "v0.0.118", "v0.0.119", "v0.0.120",
        "v0.0.121", "v0.0.122", "v0.0.123", "v0.0.124", "v0.0.125", "v0.0.126",
        "v0.0.127", "v0.0.128", "v0.0.129", "v0.0.130", "v0.0.131", "v0.0.132",
        "v0.0.133", "v0.0.134", "v0.0.135",
        # "v0.0.136",
        "v0.0.137", "v0.0.138",
        "v0.0.139", "v0.0.140", "v0.0.141", "v0.0.142", "v0.0.143", "v0.0.144",
        "v0.0.145", "v0.0.146", "v0.0.147", "v0.0.148", "v0.0.149", "v0.0.150",
        "v0.0.151", "v0.0.152", "v0.0.153", "v0.0.154", "v0.0.155", "v0.0.156",
        "v0.0.157", "v0.0.158", "v0.0.159", "v0.0.160", "v0.0.161", "v0.0.162",
        "v0.0.163", "v0.0.164", "v0.0.165", "v0.0.166", "v0.0.167", "v0.0.168",
        "v0.0.169", "v0.0.170", "v0.0.171", "v0.0.172", "v0.0.173", "v0.0.174",
        "v0.0.175", "v0.0.176", "v0.0.177", "v0.0.178", "v0.0.179", "v0.0.180",
        "v0.0.181", "v0.0.182", "v0.0.183", "v0.0.184", "v0.0.185", "v0.0.186",
        "v0.0.187", "v0.0.188", "v0.0.189", "v0.0.190", "v0.0.191", "v0.0.192",
        "v0.0.193", "v0.0.194", "v0.0.195", "v0.0.196", "v0.0.198", "v0.0.199",
        "v0.0.200", "v0.0.201", "v0.0.202", "v0.0.203", "v0.0.204", "v0.0.205",
        "v0.0.206", "v0.0.207", "v0.0.208", "v0.0.209", "v0.0.210", "v0.0.211",
        "v0.0.212", "v0.0.213", "v0.0.214", "v0.0.215", "v0.0.216", "v0.0.217",
        "v0.0.218", "v0.0.219", "v0.0.220", "v0.0.221", "v0.0.222", "v0.0.223",
        "v0.0.224", "v0.0.225", "v0.0.226", "v0.0.227", "v0.0.228", "v0.0.229",
        "v0.0.230", "v0.0.231",
        # "v0.0.232",
        "v0.0.233", "v0.0.234", "v0.0.235",
        "v0.0.236", "v0.0.237", "v0.0.238", "v0.0.239", "v0.0.240", "v0.0.241",
        "v0.0.242", "v0.0.243", "v0.0.244", "v0.0.245", "v0.0.246", "v0.0.247",
        "v0.0.248", "v0.0.249", "v0.0.250", "v0.0.251", "v0.0.252", "v0.0.253",
        "v0.0.254", "v0.0.255", "v0.0.256", "v0.0.257", "v0.0.258", "v0.0.259",
        "v0.0.260", "v0.0.261", "v0.0.262", "v0.0.263", "v0.0.264", "v0.0.265",
        "v0.0.266", "v0.0.267",
        # "v0.0.268",
        "v0.0.269", "v0.0.270", "v0.0.271",
        "v0.0.272", "v0.0.273", "v0.0.274", "v0.0.275", "v0.0.276", "v0.0.277",
        "v0.0.278", "v0.0.279", "v0.0.280", "v0.0.281", "v0.0.282", "v0.0.283",
        "v0.0.284", "v0.0.285", "v0.0.286", "v0.0.287", "v0.0.288", "v0.0.289",
        "v0.0.290", "v0.0.291", "v0.0.292", "v0.1.0", "v0.1.1", "v0.1.2",
        "v0.1.3", "v0.1.4", "v0.1.5", "v0.1.6", "v0.1.7", "v0.1.8",
        "v0.1.9", "v0.1.10", "v0.1.11", "v0.1.12", "v0.1.13", "v0.1.14",
        "v0.1.15", "v0.2.0", "v0.2.1", "v0.2.2", "v0.3.0", "v0.3.1",
        "v0.3.2", "v0.3.3", "v0.3.4", "v0.3.5", "v0.3.6", "v0.3.7",
        "v0.4.0", "v0.4.1", "v0.4.2", "v0.4.3", "v0.4.4", "v0.4.5",
        "v0.4.6", "v0.4.7", "v0.4.8", "v0.4.9", "v0.4.10", "0.5.0",
        "0.5.1", "0.5.2", "0.5.3", "0.5.4", "0.5.5", "0.5.6",
        "0.5.7", "0.6.0", "0.6.1", "0.6.2", "0.6.3", "0.6.4",
        "0.6.5", "0.6.6", "0.6.7", "0.6.8", "0.6.9", "0.7.0",
        "0.7.1", "0.7.2", "0.7.3", "0.7.4", "0.8.0", "0.8.1",
        "0.8.2", "0.8.3", "0.8.4", "0.8.5", "0.8.6", "0.9.0",
        "0.9.1", "0.9.2", "0.9.3", "0.9.4", "0.9.5", "0.9.6",
        "0.9.7", "0.9.8", "0.9.9", "0.9.10", "0.10.0", "0.11.0",
        "0.11.1", "0.11.2", "0.11.3", "0.11.4", "0.11.5", "0.11.6",
        "0.11.7", "0.11.8", "0.11.9", "0.11.10", "0.11.11", "0.11.12",
        "0.11.13", "0.12.0", "0.12.1", "0.12.2", "0.12.3", "0.12.4",
        "0.12.5", "0.12.7", "0.12.8", "0.12.9", "0.12.10", "0.12.11",
        "0.12.12", "0.13.0", "0.13.1", "0.13.2", "0.13.3", "0.14.0",
        "0.14.1",
    ]
    # fmt: on


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
        if not version:
            msg = f"  ❌ {rule_code} not found in any version"
            raise ValueError(msg)

        # Get PR information from git blame
        pr_number = None

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
