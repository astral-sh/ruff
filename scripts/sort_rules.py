#!/usr/bin/env python3
"""Generate boilerplate for a new rule.

Example usage:

    python scripts/add_rule.py \
        --linter flake8-pie
"""

import argparse
from pathlib import Path
from typing import Optional

from _utils import (
    ROOT_DIR,
    dir_name,
    get_indent,
    key_code_to_rule_pair,
    key_mod,
    key_pub_use,
    key_test_case,
)


def sort_test_cases(
    plugin_module: Path,
    nb_digit: int,
    *,
    test_case_to_add: Optional[str] = None,
) -> None:
    """Sort the `#testcase` macros."""
    mod_rs = plugin_module / "mod.rs"
    content = mod_rs.read_text()

    with mod_rs.open("w") as fp:
        has_added_testcase = False
        lines = []
        for line in content.splitlines():
            if not has_added_testcase and (
                line.strip() == "fn rules(rule_code: Rule, path: &Path) -> Result<()> {"
            ):
                if test_case_to_add is not None:
                    indent = get_indent(line)
                    lines.append(indent + test_case_to_add)
                lines.sort(key=key_test_case(nb_digit))
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

        if not has_added_testcase:
            fp.write("\n".join(lines))
            fp.write("\n")


def sort_exports(
    plugin_module: Path,
    *,
    pub_use_to_add: Optional[str] = None,
    mod_to_add: Optional[str] = None,
) -> None:
    """Sort the exports."""
    rules_mod = plugin_module / "rules" / "mod.rs"

    contents = rules_mod.read_text()
    parts = contents.split("\n\n")

    if len(parts) == 2:
        pub_use_contents = parts[0].split(";\n")
        if pub_use_to_add is not None:
            pub_use_contents.append(pub_use_to_add)
        pub_use_contents.sort(key=key_pub_use)

        mod_contents = parts[1].splitlines()
        if mod_to_add is not None:
            mod_contents.append(mod_to_add)
        mod_contents.sort(key=key_mod)

        new_contents = ";\n".join(pub_use_contents)
        new_contents += "\n\n"
        new_contents += "\n".join(mod_contents)
        new_contents += "\n"

        rules_mod.write_text(new_contents)
    else:
        if pub_use_to_add is not None and mod_to_add is not None:
            rules_mod.write_text(f"{pub_use_to_add};\n\n{mod_to_add}\n")


def sort_code_to_violation_pairs(
    linter: str,
    *,
    code_to_violation_pair_to_add: Optional[str] = None,
) -> None:
    """Sort the code-to-violation pairs from `src/registry.rs`."""
    content = (ROOT_DIR / "crates/ruff/src/registry.rs").read_text()

    seen_macro = False
    has_written = False
    has_seen_linter = False
    with (ROOT_DIR / "crates/ruff/src/registry.rs").open("w") as fp:
        lines = []
        for line in content.splitlines():
            if has_written:
                fp.write(line)
                fp.write("\n")
                continue

            if line.startswith("ruff_macros::register_rules!"):
                seen_macro = True
                fp.write(line)
                fp.write("\n")
                continue

            if not seen_macro:
                fp.write(line)
                fp.write("\n")
                continue

            if line.strip() == f"// {linter}":
                if code_to_violation_pair_to_add is not None:
                    indent = get_indent(line)
                    lines.append(indent + code_to_violation_pair_to_add)
                has_seen_linter = True
                fp.write(line)
                fp.write("\n")
                continue

            if not has_seen_linter:
                fp.write(line)
                fp.write("\n")
                continue

            if not line.strip().startswith("// "):
                lines.append(line)
            else:
                lines.sort()
                fp.write("\n".join(lines))
                fp.write("\n")
                fp.write(line)
                fp.write("\n")
                has_written = True

    assert has_written


def sort_code_to_rule_pairs(
    linter: str,
    *,
    code_to_rule_pair_to_add: Optional[str] = None,
) -> None:
    """Sort the code-to-rule pairs from `src/codes.rs`."""
    text = ""
    with (ROOT_DIR / "crates/ruff/src/codes.rs").open("r") as fp:
        while (line := next(fp)).strip() != f"// {linter}":
            text += line
        text += line

        lines = []
        while (line := next(fp)).strip() != "":
            lines.append(line)

        if code_to_rule_pair_to_add is not None:
            lines.append(code_to_rule_pair_to_add)
        lines.sort(key=key_code_to_rule_pair)

        text += "".join(lines)
        text += "\n"

        text += fp.read()

    with (ROOT_DIR / "crates/ruff/src/codes.rs").open("w") as fp:
        fp.write(text)


def sort_linter(*, linter: str, nb_digit: int) -> None:
    """Sort the code of the linter."""
    plugin_module = ROOT_DIR / "crates/ruff/src/rules" / dir_name(linter)

    sort_test_cases(plugin_module, nb_digit)
    try:
        sort_exports(plugin_module)
    except FileNotFoundError:
        print(f"'{linter}' use a deprecated rules architecture, skip sorting exports.")
    sort_code_to_violation_pairs(linter)
    sort_code_to_rule_pairs(linter)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new rule.",
        epilog=(
            "python scripts/add_rule.py "
            "--name PreferListBuiltin --code PIE807 --linter flake8-pie"
        ),
    )
    parser.add_argument(
        "--linter",
        type=str,
        required=True,
        help="The source with which the check originated (e.g., 'flake8-pie').",
    )
    parser.add_argument(
        "--nb-digit",
        type=int,
        default=3,
        help="The number of digits in the rule code (e.g., '3').",
    )
    args = parser.parse_args()

    main(linter=args.linter, nb_digit=args.nb_digit)
