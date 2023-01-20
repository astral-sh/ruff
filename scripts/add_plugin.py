#!/usr/bin/env python3
"""Generate boilerplate for a new Flake8 plugin.

Example usage:

    python scripts/add_plugin.py \
        flake8-pie \
        --url https://pypi.org/project/flake8-pie/0.16.0/
"""

import argparse
import os

from _utils import ROOT_DIR, dir_name, get_indent, pascal_case


def main(*, plugin: str, url: str) -> None:
    # Create the test fixture folder.
    os.makedirs(
        ROOT_DIR / "resources/test/fixtures" / dir_name(plugin),
        exist_ok=True,
    )

    # Create the Rust module.
    rust_module = ROOT_DIR / "src/rules" / dir_name(plugin)
    os.makedirs(rust_module, exist_ok=True)
    with open(rust_module / "rules.rs", "w+") as fp:
        fp.write("use crate::checkers::ast::Checker;\n")
    with open(rust_module / "mod.rs", "w+") as fp:
        fp.write(f"//! Rules from [{plugin}]({url}).\n")
        fp.write("pub(crate) mod rules;\n")
        fp.write("\n")
        fp.write(
            """#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::linter::test_path;
    use crate::settings;

    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics =test_path(
            Path::new("./resources/test/fixtures/%s")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
"""
            % dir_name(plugin)
        )

    # Add the plugin to `rules/mod.rs`.
    with open(ROOT_DIR / "src/rules/mod.rs", "a") as fp:
        fp.write(f"pub mod {dir_name(plugin)};")

    # Add the relevant sections to `src/registry.rs`.
    content = (ROOT_DIR / "src/registry.rs").read_text()

    with open(ROOT_DIR / "src/registry.rs", "w") as fp:
        for line in content.splitlines():
            indent = get_indent(line)

            if line.strip() == "// Ruff":
                fp.write(f"{indent}// {plugin}")
                fp.write("\n")

            elif line.strip() == "Ruff,":
                fp.write(f"{indent}{pascal_case(plugin)},")
                fp.write("\n")

            elif line.strip() == "RuleOrigin::Ruff => Prefixes::Single(RuleCodePrefix::RUF),":
                prefix = 'todo!("Fill-in prefix after generating codes")'
                fp.write(
                    f"{indent}RuleOrigin::{pascal_case(plugin)} => Prefixes::Single({prefix}),"
                )
                fp.write("\n")

            fp.write(line)
            fp.write("\n")

    # Add the relevant section to `src/violations.rs`.
    content = (ROOT_DIR / "src/violations.rs").read_text()

    with open(ROOT_DIR / "src/violations.rs", "w") as fp:
        for line in content.splitlines():
            if line.strip() == "// Ruff":
                indent = get_indent(line)
                fp.write(f"{indent}// {plugin}")
                fp.write("\n")

            fp.write(line)
            fp.write("\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new Flake8 plugin.",
        epilog=(
            "Example usage: python scripts/add_plugin.py flake8-pie "
            "--url https://pypi.org/project/flake8-pie/0.16.0/"
        ),
    )
    parser.add_argument(
        "plugin",
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
