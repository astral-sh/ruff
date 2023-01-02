"""Generate boilerplate for a new plugin.

Example usage:

    python scripts/add_check.py \
        flake8-pie \
        --url https://pypi.org/project/flake8-pie/0.16.0/
"""

import argparse
import os

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def dir_name(plugin: str) -> str:
    return plugin.replace("-", "_")


def pascal_case(plugin: str) -> str:
    return "".join(word.title() for word in plugin.split("-"))


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

    # Add the relevant sections to `src/registry.rs`.
    with open(os.path.join(ROOT_DIR, "src/registry.rs"), "r") as fp:
        content = fp.read()

    index = 0
    with open(os.path.join(ROOT_DIR, "src/registry.rs"), "w") as fp:
        for line in content.splitlines():
            fp.write(line)
            fp.write("\n")

            if line.strip() == f"// {dir_name(plugin)}":
                if index == 0:
                    # `CheckCode` definition
                    indent = line.split(f"// {dir_name(plugin)}")[0]
                    fp.write(f"{indent}{code},")
                    fp.write("\n")

                elif index == 1:
                    # `CheckKind` definition
                    indent = line.split(f"// {dir_name(plugin)}")[0]
                    fp.write(f"{indent}{pascal_case(name)},")

                elif index == 2:
                    # `CheckCode#kind()`
                    indent = line.split(f"// {dir_name(plugin)}")[0]
                    fp.write(f"{indent}CheckCode::{code} => CheckKind::{pascal_case(name)},")

                elif index == 3:
                    # `CheckKind#code()`
                    indent = line.split(f"// {dir_name(plugin)}")[0]
                    fp.write(f"{indent}CheckKind::{pascal_case(name)} => &CheckCode::{code},")

                elif index == 4:
                    # `CheckCode#body`
                    indent = line.split(f"// {dir_name(plugin)}")[0]
                    fp.write(f'{indent}CheckKind::{pascal_case(name)} => "".to_string(),')

                index += 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new plugin.",
        epilog=(
            "Example usage: python scripts/add_check.py flake8-pie --url https://pypi.org/project/flake8-pie/0.16.0/"
        ),
    )
    parser.add_argument(
        "--name",
        type=str,
        help="The name of the check to generate, in PascalCase (e.g., 'LineTooLong').",
    )
    parser.add_argument(
        "--code",
        type=str,
        help="The code of the check to generate (e.g., 'A001').",
    )
    parser.add_argument(
        "--plugin",
        type=str,
        help="The plugin with which the check is associated (e.g., 'flake8-builtins').",
    )
    args = parser.parse_args()

    main(name=args.name, code=args.code, plugin=args.plugin)
