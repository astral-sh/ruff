#!/usr/bin/env python3
"""Generate boilerplate for a new rule.

Example usage:

    python scripts/add_rule.py \
        --name PreferListBuiltin \
        --prefix PIE \
        --code 807 \
        --linter flake8-pie
"""

from __future__ import annotations

import argparse
import subprocess

from _utils import ROOT_DIR, dir_name, get_indent, pascal_case, snake_case


def main(*, name: str, prefix: str, code: str, linter: str) -> None:
    """Generate boilerplate for a new rule."""
    # Create a test fixture.
    filestem = f"{prefix}{code}" if linter != "pylint" else snake_case(name)
    with (
        ROOT_DIR
        / "crates/ruff_linter/resources/test/fixtures"
        / dir_name(linter)
        / f"{filestem}.py"
    ).open("a"):
        pass

    plugin_module = ROOT_DIR / "crates/ruff_linter/src/rules" / dir_name(linter)
    rule_name_snake = snake_case(name)

    # Add the relevant `#testcase` macro.
    mod_rs = plugin_module / "mod.rs"
    content = mod_rs.read_text()

    with mod_rs.open("w") as fp:
        has_added_testcase = False
        lines = []
        for line in content.splitlines():
            if not has_added_testcase and (
                line.strip() == "fn rules(rule_code: Rule, path: &Path) -> Result<()> {"
            ):
                indent = get_indent(line)
                lines.append(
                    f'{indent}#[test_case(Rule::{name}, Path::new("{filestem}.py"))]',
                )
                fp.write("\n".join(lines))
                fp.write("\n")
                lines.clear()
                has_added_testcase = True

            if has_added_testcase:
                fp.write(line)
                fp.write("\n")
            elif line.strip() == "":
                fp.write("\n".join(lines))
                fp.write("\n\n")
                lines.clear()
            else:
                lines.append(line)

    # Add the exports
    rules_dir = plugin_module / "rules"
    rules_mod = rules_dir / "mod.rs"

    contents = rules_mod.read_text()
    parts = contents.split("\n\n")

    new_pub_use = f"pub(crate) use {rule_name_snake}::*"
    new_mod = f"mod {rule_name_snake};"

    if len(parts) == 2:
        new_contents = parts[0]
        new_contents += "\n" + new_pub_use + ";"
        new_contents += "\n\n"
        new_contents += parts[1] + new_mod
        new_contents += "\n"

        rules_mod.write_text(new_contents)
    else:
        with rules_mod.open("a") as fp:
            fp.write(f"{new_pub_use};")
            fp.write("\n\n")
            fp.write(f"{new_mod}")
            fp.write("\n")

    # Add the relevant rule function.
    with (rules_dir / f"{rule_name_snake}.rs").open("w") as fp:
        fp.write(
            f"""\
use ruff_diagnostics::Violation;
use ruff_macros::{{derive_message_formats, violation}};

use crate::checkers::ast::Checker;

/// ## What it does
///
/// ## Why is this bad?
///
/// ## Example
/// ```python
/// ```
///
/// Use instead:
/// ```python
/// ```
#[violation]
pub struct {name};

impl Violation for {name} {{
    #[derive_message_formats]
    fn message(&self) -> String {{
        format!("TODO: write message: {{}}", todo!("implement message"))
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

    text = ""
    with (ROOT_DIR / "crates/ruff_linter/src/codes.rs").open("r") as fp:
        while (line := next(fp)).strip() != f"// {linter}":
            text += line
        text += line

        lines = []
        while (line := next(fp)).strip() != "":
            lines.append(line)

        variant = pascal_case(linter)
        linter_name = linter.split(" ")[0].replace("-", "_")
        rule = f"""rules::{linter_name}::rules::{name}"""
        lines.append(
            " " * 8 + f"""({variant}, "{code}") => (RuleGroup::Preview, {rule}),\n""",
        )
        lines.sort()
        text += "".join(lines)
        text += "\n"
        text += fp.read()
    with (ROOT_DIR / "crates/ruff_linter/src/codes.rs").open("w") as fp:
        fp.write(text)

    _rustfmt(rules_mod)


def _rustfmt(path: str) -> None:
    subprocess.run(["rustfmt", path])


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
