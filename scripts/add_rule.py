#!/usr/bin/env python3
"""Generate boilerplate for a new rule.

Example usage:

    python scripts/add_rule.py \
        --name PreferListBuiltin \
        --code PIE807 \
        --linter flake8-pie
"""

import argparse

from _utils import ROOT_DIR, dir_name, get_indent


def snake_case(name: str) -> str:
    """Convert from PascalCase to snake_case."""
    return "".join(
        f"_{word.lower()}" if word.isupper() else word for word in name
    ).lstrip("_")


def main(*, name: str, code: str, linter: str) -> None:
    """Generate boilerplate for a new rule."""
    # Create a test fixture.
    with (ROOT_DIR / "resources/test/fixtures" / dir_name(linter) / f"{code}.py").open(
        "a",
    ):
        pass

    plugin_module = ROOT_DIR / "src/rules" / dir_name(linter)
    rule_name_snake = snake_case(name)

    # Add the relevant `#testcase` macro.
    mod_rs = plugin_module / "mod.rs"
    content = mod_rs.read_text()

    with mod_rs.open("w") as fp:
        for line in content.splitlines():
            if line.strip() == "fn rules(rule_code: Rule, path: &Path) -> Result<()> {":
                indent = get_indent(line)
                fp.write(
                    f'{indent}#[test_case(Rule::{name}, Path::new("{code}.py"); "{code}")]',
                )
                fp.write("\n")

            fp.write(line)
            fp.write("\n")

    # Add the exports
    rules_dir = plugin_module / "rules"
    rules_mod = rules_dir / "mod.rs"

    contents = rules_mod.read_text()
    parts = contents.split("\n\n")
    if len(parts) == 2:
        new_contents = parts[0] + "\n"
        new_contents += f"pub use {rule_name_snake}::{{{rule_name_snake}, {name}}};"
        new_contents += "\n"
        new_contents += "\n"
        new_contents += parts[1]
        new_contents += f"mod {rule_name_snake};"
        new_contents += "\n"
        rules_mod.write_text(new_contents)
    else:
        with rules_mod.open("a") as fp:
            fp.write(f"pub use {rule_name_snake}::{{{rule_name_snake}, {name}}};")
            fp.write("\n")
            fp.write(f"mod {rule_name_snake};")
            fp.write("\n")

    # Add the relevant rule function.
    with (rules_dir / f"{rule_name_snake}.rs").open("w") as fp:
        fp.write(
            """use ruff_macros::derive_message_formats;

use crate::define_violation;
use crate::violation::Violation;
use crate::checkers::ast::Checker;

define_violation!(
    pub struct %s;
);
impl Violation for %s {
    #[derive_message_formats]
    fn message(&self) -> String {
        todo!("implement message");
        format!("TODO: write message")
    }
}
"""
            % (name, name),
        )
        fp.write("\n")
        fp.write(
            f"""
/// {code}
pub fn {rule_name_snake}(checker: &mut Checker) {{}}
""",
        )
        fp.write("\n")

    # Add the relevant code-to-violation pair to `src/registry.rs`.
    content = (ROOT_DIR / "src/registry.rs").read_text()

    seen_macro = False
    has_written = False
    with (ROOT_DIR / "src/registry.rs").open("w") as fp:
        for line in content.splitlines():
            fp.write(line)
            fp.write("\n")

            if has_written:
                continue

            if line.startswith("ruff_macros::define_rule_mapping!"):
                seen_macro = True
                continue

            if not seen_macro:
                continue

            if line.strip() == f"// {linter}":
                indent = get_indent(line)
                fp.write(f"{indent}{code} => rules::{dir_name(linter)}::rules::{name},")
                fp.write("\n")
                has_written = True

    assert has_written


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new rule.",
        epilog="python scripts/add_rule.py --name PreferListBuiltin --code PIE807 --linter flake8-pie",
    )
    parser.add_argument(
        "--name",
        type=str,
        required=True,
        help="The name of the check to generate, in PascalCase (e.g., 'LineTooLong').",
    )
    parser.add_argument(
        "--code",
        type=str,
        required=True,
        help="The code of the check to generate (e.g., 'A001').",
    )
    parser.add_argument(
        "--linter",
        type=str,
        required=True,
        help="The source with which the check originated (e.g., 'flake8-builtins').",
    )
    args = parser.parse_args()

    main(name=args.name, code=args.code, linter=args.linter)
