#!/usr/bin/env python3
"""Generate boilerplate for a new rule.

Example usage:

    python scripts/add_rule.py \
        --name PreferListBuiltin \
        --code PIE807 \
        --origin flake8-pie
"""

import argparse

from _utils import ROOT_DIR, dir_name, get_indent


def snake_case(name: str) -> str:
    """Convert from PascalCase to snake_case."""
    return "".join(f"_{word.lower()}" if word.isupper() else word for word in name).lstrip("_")


def main(*, name: str, code: str, origin: str) -> None:
    # Create a test fixture.
    with open(
        ROOT_DIR / "resources/test/fixtures" / dir_name(origin) / f"{code}.py",
        "a",
    ):
        pass

    # Add the relevant `#testcase` macro.
    mod_rs = ROOT_DIR / "src/rules" / dir_name(origin) / "mod.rs"
    content = mod_rs.read_text()

    with open(mod_rs, "w") as fp:
        for line in content.splitlines():
            if line.strip() == "fn rules(rule_code: Rule, path: &Path) -> Result<()> {":
                indent = get_indent(line)
                fp.write(f'{indent}#[test_case(Rule::{code}, Path::new("{code}.py"); "{code}")]')
                fp.write("\n")

            fp.write(line)
            fp.write("\n")

    # Add the relevant rule function.
    with open(ROOT_DIR / "src/rules" / dir_name(origin) / (snake_case(name) + ".rs"), "w") as fp:
        fp.write(
            f"""
/// {code}
pub fn {snake_case(name)}(checker: &mut Checker) {{}}
"""
        )
        fp.write("\n")

    # Add the relevant struct to `src/violations.rs`.
    content = (ROOT_DIR / "src/violations.rs").read_text()

    with open(ROOT_DIR / "src/violations.rs", "w") as fp:
        for line in content.splitlines():
            fp.write(line)
            fp.write("\n")

            if line.startswith(f"// {origin}"):
                fp.write(
                    """define_violation!(
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
                    % (name, name)
                )
                fp.write("\n")

    # Add the relevant code-to-violation pair to `src/registry.rs`.
    content = (ROOT_DIR / "src/registry.rs").read_text()

    seen_macro = False
    has_written = False
    with open(ROOT_DIR / "src/registry.rs", "w") as fp:
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

            if line.strip() == f"// {origin}":
                indent = get_indent(line)
                fp.write(f"{indent}{code} => violations::{name},")
                fp.write("\n")
                has_written = True

    assert has_written


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new rule.",
        epilog="python scripts/add_rule.py --name PreferListBuiltin --code PIE807 --origin flake8-pie",
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
        "--origin",
        type=str,
        required=True,
        help="The source with which the check originated (e.g., 'flake8-builtins').",
    )
    args = parser.parse_args()

    main(name=args.name, code=args.code, origin=args.origin)
