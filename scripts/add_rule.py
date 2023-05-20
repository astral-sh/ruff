#!/usr/bin/env python3
"""Generate boilerplate for a new rule.

Example usage:

    python scripts/add_rule.py \
        --name PreferListBuiltin \
        --prefix PIE \
        --code 807 \
        --linter flake8-pie
"""

import argparse
import re
from pathlib import Path

from _utils import ROOT_DIR, dir_name, pascal_case, snake_case
from sort_rules import (
    sort_code_to_rule_pairs,
    sort_code_to_violation_pairs,
    sort_exports,
    sort_test_cases,
)


def create_test_fixture(prefix: str, code: str, linter: str) -> None:
    """Create a test fixture for a new rule."""
    with (
        ROOT_DIR
        / "crates/ruff/resources/test/fixtures"
        / dir_name(linter)
        / f"{prefix}{code}.py"
    ).open(
        "a",
    ):
        pass


def add_test_case(
    plugin_module: Path,
    name: str,
    prefix: str,
    code: str,
    linter: str,
) -> None:
    """Add the relevant `#testcase` macro."""
    filestem = f"{prefix}{code}" if linter != "pylint" else snake_case(name)
    test_case_to_add = (
        f'#[test_case(Rule::{name}, Path::new("{filestem}.py"); "{prefix}{code}")]'
    )
    m = re.search(r"\d", code)
    nb_digit = len(code) - (m.start() if m else 0)
    sort_test_cases(plugin_module, nb_digit, test_case_to_add=test_case_to_add)


def add_exports(rules_dir: Path, name: str, linter: str) -> None:
    """Add the exports."""
    rule_name_snake = snake_case(name)
    new_pub_use = f"pub(crate) use {rule_name_snake}::{{{rule_name_snake}, {name}}}"
    new_mod = f"mod {rule_name_snake};"

    try:
        sort_exports(rules_dir, pub_use_to_add=new_pub_use, mod_to_add=new_mod)
    except FileNotFoundError:
        print(
            f"'{linter}' use a deprecated rules architecture.\n"
            "Please update the rules architecture to use the new one",
        )


def add_rule_function(
    rules_dir: Path,
    name: str,
    prefix: str,
    code: str,
) -> None:
    """Add the relevant rule function."""
    rule_name_snake = snake_case(name)
    with (rules_dir / f"{rule_name_snake}.rs").open("w") as fp:
        fp.write(
            f"""\
use ruff_diagnostics::Violation;
use ruff_macros::{{derive_message_formats, violation}};

use crate::checkers::ast::Checker;

#[violation]
pub struct {name};

impl Violation for {name} {{
    #[derive_message_formats]
    fn message(&self) -> String {{
        todo!("implement message");
        format!("TODO: write message")
    }}
}}
""",
        )
        fp.write(
            f"""
/// {prefix}{code}
pub(crate) fn {rule_name_snake}(checker: &mut Checker) {{}}
""",
        )


def add_code_to_violation_pair(
    name: str,
    linter: str,
) -> None:
    """Add the relevant code-to-violation pair to `src/registry.rs`."""
    new_code_to_violation_pair = f"rules::{dir_name(linter)}::rules::{name},"
    sort_code_to_violation_pairs(
        linter,
        code_to_violation_pair_to_add=new_code_to_violation_pair,
    )


def add_code_to_rule_pair(
    name: str,
    code: str,
    linter: str,
) -> None:
    """Add the relevant code-to-rule pair to `src/codes.rs`."""
    variant = pascal_case(linter)
    new_code_to_rule_pair = (
        " " * 8 + f'({variant}, "{code}") => (RuleGroup::Unspecified, Rule::{name}),\n'
    )
    sort_code_to_rule_pairs(linter, code_to_rule_pair_to_add=new_code_to_rule_pair)


def main(*, name: str, prefix: str, code: str, linter: str) -> None:
    """Generate boilerplate for a new rule."""
    plugin_module = ROOT_DIR / "crates/ruff/src/rules" / dir_name(linter)
    rules_dir = plugin_module / "rules"

    create_test_fixture(prefix, code, linter)
    add_test_case(plugin_module, name, prefix, code, linter)
    add_exports(rules_dir, name, linter)
    add_rule_function(rules_dir, name, prefix, code)
    add_code_to_violation_pair(name, linter)
    add_code_to_rule_pair(name, code, linter)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new rule.",
        epilog=(
            "python scripts/add_rule.py "
            "--name PreferListBuiltin --code PIE807 --linter flake8-pie"
        ),
    )
    parser.add_argument(
        "--name",
        type=str,
        required=True,
        help=(
            "The name of the check to generate, in PascalCase "
            "(e.g., 'PreferListBuiltin')."
        ),
    )
    parser.add_argument(
        "--prefix",
        type=str,
        required=True,
        help="Prefix code for the plugin (e.g. 'PIE').",
    )
    parser.add_argument(
        "--code",
        type=str,
        required=True,
        help="The code of the check to generate (e.g., '807').",
    )
    parser.add_argument(
        "--linter",
        type=str,
        required=True,
        help="The source with which the check originated (e.g., 'flake8-pie').",
    )
    args = parser.parse_args()

    main(name=args.name, prefix=args.prefix, code=args.code, linter=args.linter)
