#!/usr/bin/env python3
"""Generate boilerplate for a new check.

Example usage:

    python scripts/add_check.py \
        --name PreferListBuiltin \
        --code PIE807 \
        --plugin flake8-pie
"""

import argparse
import os

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def dir_name(plugin: str) -> str:
    return plugin.replace("-", "_")


def pascal_case(plugin: str) -> str:
    """Convert from snake-case to PascalCase."""
    return "".join(word.title() for word in plugin.split("-"))


def snake_case(name: str) -> str:
    """Convert from PascalCase to snake_case."""
    return "".join(f"_{word.lower()}" if word.isupper() else word for word in name).lstrip("_")


def main(*, name: str, code: str, plugin: str) -> None:
    # Create a test fixture.
    with open(
        os.path.join(ROOT_DIR, f"resources/test/fixtures/{dir_name(plugin)}/{code}.py"),
        "a",
    ):
        pass

    # Add the relevant `#testcase` macro.
    with open(os.path.join(ROOT_DIR, f"src/{dir_name(plugin)}/mod.rs"), "r") as fp:
        content = fp.read()

    with open(os.path.join(ROOT_DIR, f"src/{dir_name(plugin)}/mod.rs"), "w") as fp:
        for line in content.splitlines():
            if line.strip() == "fn checks(check_code: CheckCode, path: &Path) -> Result<()> {":
                indent = line.split("fn checks(check_code: CheckCode, path: &Path) -> Result<()> {")[0]
                fp.write(f'{indent}#[test_case(CheckCode::{code}, Path::new("{code}.py"); "{code}")]')
                fp.write("\n")

            fp.write(line)
            fp.write("\n")

    # Add the relevant plugin function.
    with open(os.path.join(ROOT_DIR, f"src/{dir_name(plugin)}/plugins.rs"), "a") as fp:
        fp.write(
            f"""
/// {code}
pub fn {snake_case(name)}(checker: &mut Checker) {{}}
"""
        )
        fp.write("\n")

    # Add the relevant sections to `src/registry.rs`.
    with open(os.path.join(ROOT_DIR, "src/registry.rs"), "r") as fp:
        content = fp.read()

    index = 0
    with open(os.path.join(ROOT_DIR, "src/registry.rs"), "w") as fp:
        for line in content.splitlines():
            fp.write(line)
            fp.write("\n")

            if line.strip() == f"// {plugin}":
                if index == 0:
                    # `CheckCode` definition
                    indent = line.split(f"// {plugin}")[0]
                    fp.write(f"{indent}{code},")
                    fp.write("\n")

                elif index == 1:
                    # `CheckKind` definition
                    indent = line.split(f"// {plugin}")[0]
                    fp.write(f"{indent}{name},")
                    fp.write("\n")

                elif index == 2:
                    # `CheckCode#kind()`
                    indent = line.split(f"// {plugin}")[0]
                    fp.write(f"{indent}CheckCode::{code} => CheckKind::{name},")
                    fp.write("\n")

                elif index == 3:
                    # `CheckCode#category()`
                    indent = line.split(f"// {plugin}")[0]
                    fp.write(f"{indent}CheckCode::{code} => CheckCategory::{pascal_case(plugin)},")
                    fp.write("\n")

                elif index == 4:
                    # `CheckKind#code()`
                    indent = line.split(f"// {plugin}")[0]
                    fp.write(f"{indent}CheckKind::{name} => &CheckCode::{code},")
                    fp.write("\n")

                elif index == 5:
                    # `CheckCode#body`
                    indent = line.split(f"// {plugin}")[0]
                    fp.write(f'{indent}CheckKind::{name} => todo!("Write message body for {code}"),')
                    fp.write("\n")

                index += 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new check.",
        epilog="python scripts/add_check.py --name PreferListBuiltin --code PIE807 --plugin flake8-pie",
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
        "--plugin",
        type=str,
        required=True,
        help="The plugin with which the check is associated (e.g., 'flake8-builtins').",
    )
    args = parser.parse_args()

    main(name=args.name, code=args.code, plugin=args.plugin)
