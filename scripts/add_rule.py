#!/usr/bin/env python3
"""Generate boilerplate for a new rule.

Example usage:

    python scripts/add_rule.py \
        --name PreferListBuiltin \
        --code PIE807 \
        --origin flake8-pie
"""

import argparse
import os

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def dir_name(origin: str) -> str:
    return origin.replace("-", "_")


def pascal_case(origin: str) -> str:
    """Convert from snake-case to PascalCase."""
    return "".join(word.title() for word in origin.split("-"))


def snake_case(name: str) -> str:
    """Convert from PascalCase to snake_case."""
    return "".join(f"_{word.lower()}" if word.isupper() else word for word in name).lstrip("_")


def main(*, name: str, code: str, origin: str) -> None:
    # Create a test fixture.
    with open(
        os.path.join(ROOT_DIR, f"resources/test/fixtures/{dir_name(origin)}/{code}.py"),
        "a",
    ):
        pass

    # Add the relevant `#testcase` macro.
    with open(os.path.join(ROOT_DIR, f"src/{dir_name(origin)}/mod.rs")) as fp:
        content = fp.read()

    with open(os.path.join(ROOT_DIR, f"src/{dir_name(origin)}/mod.rs"), "w") as fp:
        for line in content.splitlines():
            if line.strip() == "fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {":
                indent = line.split("fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {")[0]
                fp.write(f'{indent}#[test_case(RuleCode::{code}, Path::new("{code}.py"); "{code}")]')
                fp.write("\n")

            fp.write(line)
            fp.write("\n")

    # Add the relevant rule function.
    with open(os.path.join(ROOT_DIR, f"src/{dir_name(origin)}/rules.rs"), "a") as fp:
        fp.write(
            f"""
/// {code}
pub fn {snake_case(name)}(checker: &mut Checker) {{}}
"""
        )
        fp.write("\n")

    # Add the relevant struct to `src/violations.rs`.
    with open(os.path.join(ROOT_DIR, "src/violations.rs")) as fp:
        content = fp.read()

    with open(os.path.join(ROOT_DIR, "src/violations.rs"), "w") as fp:
        for line in content.splitlines():
            fp.write(line)
            fp.write("\n")

            if line.startswith(f"// {origin}"):
                fp.write(
                    """define_violation!(
    pub struct %s;
);
impl Violation for %s {
    fn message(&self) -> String {
        todo!("Implement message")
    }

    fn placeholder() -> Self {
        %s
    }
}
"""
                    % (name, name, name)
                )
                fp.write("\n")

    # Add the relevant code-to-violation pair to `src/registry.rs`.
    with open(os.path.join(ROOT_DIR, "src/registry.rs")) as fp:
        content = fp.read()

    seen_macro = False
    has_written = False
    with open(os.path.join(ROOT_DIR, "src/registry.rs"), "w") as fp:
        for line in content.splitlines():
            fp.write(line)
            fp.write("\n")

            if has_written:
                continue

            if line.startswith("define_rule_mapping!"):
                seen_macro = True
                continue

            if not seen_macro:
                continue

            if line.strip() == f"// {origin}":
                indent = line.split("//")[0]
                fp.write(f"{indent}{code} => violations::{name},")
                fp.write("\n")
                has_written = True


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
