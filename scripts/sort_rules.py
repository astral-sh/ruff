#!/usr/bin/env python3
"""Generate boilerplate for a new rule.

Example usage:

    python scripts/add_rule.py \
        --linter flake8-pie
"""

import argparse

from _utils import ROOT_DIR, dir_name, key_mod, key_pub_use


def main(*, linter: str) -> None:
    """Generate boilerplate for a new rule."""
    plugin_module = ROOT_DIR / "crates/ruff/src/rules" / dir_name(linter)

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
                lines.sort(
                    key=lambda line: line.split('Path::new("')[1]
                    if linter != "pylint"
                    else line.split(");")[1],
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

    pub_use_contents = parts[0].split(";\n")
    pub_use_contents.sort(key=key_pub_use)

    mod_contents = parts[1].splitlines()
    mod_contents.sort(key=key_mod)

    new_contents = ";\n".join(pub_use_contents)
    new_contents += "\n\n"
    new_contents += "\n".join(mod_contents)
    new_contents += "\n"

    rules_mod.write_text(new_contents)

    # Add the relevant code-to-violation pair to `src/registry.rs`.
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

    text = ""
    with (ROOT_DIR / "crates/ruff/src/codes.rs").open("r") as fp:
        while (line := next(fp)).strip() != f"// {linter}":
            text += line
        text += line

        lines = []
        while (line := next(fp)).strip() != "":
            lines.append(line)

        lines.sort()

        text += "".join(lines)
        text += "\n"

        text += fp.read()

    with (ROOT_DIR / "crates/ruff/src/codes.rs").open("w") as fp:
        fp.write(text)


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
    args = parser.parse_args()

    main(linter=args.linter)
