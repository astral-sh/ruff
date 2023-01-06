#!/usr/bin/env python3
"""Generate boilerplate for a new plugin.

Example usage:

    python scripts/add_plugin.py \
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


def main(*, plugin: str, url: str) -> None:
    # Create the test fixture folder.
    os.makedirs(
        os.path.join(ROOT_DIR, f"resources/test/fixtures/{dir_name(plugin)}"),
        exist_ok=True,
    )

    # Create the Rust module.
    os.makedirs(os.path.join(ROOT_DIR, f"src/{dir_name(plugin)}"), exist_ok=True)
    with open(os.path.join(ROOT_DIR, f"src/{dir_name(plugin)}/plugins.rs"), "a"):
        pass
    with open(os.path.join(ROOT_DIR, f"src/{dir_name(plugin)}/mod.rs"), "w+") as fp:
        fp.write("pub mod plugins;\n")
        fp.write("\n")
        fp.write(
            """#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/%s")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
"""
            % dir_name(plugin)
        )

    # Add the plugin to `lib.rs`.
    with open(os.path.join(ROOT_DIR, "src/lib.rs"), "a") as fp:
        fp.write(f"pub mod {dir_name(plugin)};")

    # Add the relevant sections to `src/registry.rs`.
    with open(os.path.join(ROOT_DIR, "src/registry.rs"), "r") as fp:
        content = fp.read()

    with open(os.path.join(ROOT_DIR, "src/registry.rs"), "w") as fp:
        for line in content.splitlines():
            if line.strip() == "// Ruff":
                indent = line.split("// Ruff")[0]
                fp.write(f"{indent}// {plugin}")
                fp.write("\n")

            elif line.strip() == "Ruff,":
                indent = line.split("Ruff,")[0]
                fp.write(f"{indent}{pascal_case(plugin)},")
                fp.write("\n")

            elif line.strip() == 'CheckCategory::Ruff => "Ruff-specific rules",':
                indent = line.split('CheckCategory::Ruff => "Ruff-specific rules",')[0]
                fp.write(f'{indent}CheckCategory::{pascal_case(plugin)} => "{plugin}",')
                fp.write("\n")

            elif line.strip() == "CheckCategory::Ruff => vec![CheckCodePrefix::RUF],":
                indent = line.split("CheckCategory::Ruff => vec![CheckCodePrefix::RUF],")[0]
                fp.write(
                    f"{indent}CheckCategory::{pascal_case(plugin)} => vec![\n"
                    f'{indent}    todo!("Fill-in prefix after generating codes")\n'
                    f"{indent}],"
                )
                fp.write("\n")

            elif line.strip() == "CheckCategory::Ruff => None,":
                indent = line.split("CheckCategory::Ruff => None,")[0]
                fp.write(f"{indent}CheckCategory::{pascal_case(plugin)} => " f'Some(("{url}", &Platform::PyPI)),')
                fp.write("\n")

            fp.write(line)
            fp.write("\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new plugin.",
        epilog=(
            "Example usage: python scripts/add_plugin.py flake8-pie "
            "--url https://pypi.org/project/flake8-pie/0.16.0/"
        ),
    )
    parser.add_argument(
        "plugin",
        required=True,
        type=str,
        help="The name of the plugin to generate.",
    )
    parser.add_argument(
        "--url",
        required=True,
        type=str,
        help="The URL of the latest release in PyPI.",
    )
    args = parser.parse_args()

    main(plugin=args.plugin, url=args.url)
