#!/usr/bin/env python3
"""Generate boilerplate for a new rule.

Example usage:

    python scripts/add_rule.py \
        --linter flake8-pie
"""

import argparse
import re
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
    print(f"Sort '{linter}'.")
    plugin_module = ROOT_DIR / "crates/ruff/src/rules" / dir_name(linter)

    sort_test_cases(plugin_module, nb_digit)
    try:
        sort_exports(plugin_module)
    except FileNotFoundError:
        print(f"'{linter}' use a deprecated rules architecture, skip sorting exports.")
    sort_code_to_rule_pairs(linter)


def get_linters() -> dict[str, int]:
    """Get the linters."""
    linters = {}
    lines = (ROOT_DIR / "crates/ruff/src/codes.rs").read_text().splitlines()
    linter = None
    for line in lines:
        m = re.match(r"^        // ([^,]*)$", line)
        if m:
            linter = m.group(1)
        elif linter is not None:
            m = re.match(r'^        \([A-Za-z0-9]+, "[A-Z]?([0-9]+)"', line)
            if m:
                nb_digit = len(m.group(1))
                linters[linter] = nb_digit
                linter = None
    return linters


def main() -> None:
    """Sort the code of linters."""
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
        help="The source with which the check originated (e.g., 'flake8-pie').",
    )
    args = parser.parse_args()

    linters = get_linters()

    if args.linter is None:
        for linter, nb_digit in linters.items():
            sort_linter(linter=linter, nb_digit=nb_digit)
        return

    nb_digit = linters.get(args.linter)
    if nb_digit is None:
        print(f"Unknown linter '{args.linter}'.")
        return

    sort_linter(linter=args.linter, nb_digit=nb_digit)


if __name__ == "__main__":
    main()
