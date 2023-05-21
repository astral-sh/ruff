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

from _utils import (
    ROOT_DIR,
    RULES_DIR,
    dir_name,
    pascal_case,
    snake_case,
)
from sort_rules import (
    sort_code_to_rule_pairs,
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
    test_case_to_add = f'#[test_case(Rule::{name}, Path::new("{filestem}.py"))]'
    m = re.search(r"\d", code)
    nb_digit = len(code) - (m.start() if m else 0)
    sort_test_cases(plugin_module, nb_digit, test_case_to_add=test_case_to_add)


def add_exports(plugin_module: Path, name: str, should_create: bool) -> None:
    """Add the exports."""
    rule_name_snake = snake_case(name)
    new_pub_use = f"pub(crate) use {rule_name_snake}::{{{rule_name_snake}, {name}}}"
    new_mod = f"mod {rule_name_snake};"

    return sort_exports(
        plugin_module,
        pub_use_to_add=new_pub_use,
        mod_to_add=new_mod,
        should_create=should_create,
    )


def add_rule_function(
    plugin_module: Path,
    name: str,
    prefix: str,
    code: str,
) -> None:
    """Add the relevant rule function."""
    rule_name_snake = snake_case(name)
    rule_path = plugin_module / "rules"
    (rule_path / f"{rule_name_snake}.rs").write_text(
        f"""\
use ruff_diagnostics::Violation;
use ruff_macros::{{derive_message_formats, violation}};

use crate::checkers::ast::Checker;

#[violation]
pub struct {name};

impl Violation for {name} {{
    #[derive_message_formats]
    fn message(&self) -> String {{
        format!("TODO: write message: {{}}", todo!("implement message"))
    }}
}}
/// {prefix}{code}
pub(crate) fn {rule_name_snake}(checker: &mut Checker) {{}}
""",
    )


def add_code_to_rule_pair(
    name: str,
    code: str,
    linter: str,
) -> None:
    """Add the relevant code-to-rule pair to `src/codes.rs`."""
    variant = pascal_case(linter)
    rule = f"""rules::{linter.split(" ")[0]}::rules::{name}"""
    new_code_to_rule_pair = (
        " " * 8 + f'({variant}, "{code}") => (RuleGroup::Unspecified, {rule}),\n'
    )
    sort_code_to_rule_pairs(linter, code_to_rule_pair_to_add=new_code_to_rule_pair)


def add_rule(
    *,
    name: str,
    prefix: str,
    code: str,
    linter: str,
    # TODO(jonathan): Remove when all linters use the new architecture.
    should_create: bool,
) -> None:
    """Generate boilerplate for a new rule."""
    print(f"Adding rule '{name} ({prefix}{code})' to '{linter}'...")
    plugin_module = RULES_DIR / dir_name(linter)

    if not add_exports(plugin_module, name, should_create):
        print(f"'{linter}' use a deprecated rules architecture, skipping generation.")
        return
    add_rule_function(plugin_module, name, prefix, code)
    create_test_fixture(prefix, code, linter)
    add_test_case(plugin_module, name, prefix, code, linter)
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

    add_rule(
        name=args.name,
        prefix=args.prefix,
        code=args.code,
        linter=args.linter,
        should_create=True,
    )
