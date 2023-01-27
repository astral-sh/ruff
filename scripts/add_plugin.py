#!/usr/bin/env python3
"""Generate boilerplate for a new Flake8 plugin.

Example usage:

    python scripts/add_plugin.py \
        flake8-pie \
        --url https://pypi.org/project/flake8-pie/
        --prefix PIE
"""

import argparse

from _utils import ROOT_DIR, dir_name, get_indent, pascal_case


def main(*, plugin: str, url: str, prefix_code: str) -> None:
    """Generate boilerplate for a new plugin."""
    # Create the test fixture folder.
    (ROOT_DIR / "resources/test/fixtures" / dir_name(plugin)).mkdir(
        exist_ok=True,
    )

    # Create the Plugin rules module.
    plugin_dir = ROOT_DIR / "src/rules" / dir_name(plugin)
    plugin_dir.mkdir(exist_ok=True)

    with (plugin_dir / "mod.rs").open("w+") as fp:
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
    use crate::{assert_yaml_snapshot, settings};

    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics =test_path(
            Path::new("./resources/test/fixtures/%s")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
"""
            % dir_name(plugin),
        )

    # Create a subdirectory for rules and create a `mod.rs` placeholder
    rules_dir = plugin_dir / "rules"
    rules_dir.mkdir(exist_ok=True)

    with (rules_dir / "mod.rs").open("w+") as fp:
        fp.write("\n\n")

    # Create the snapshots subdirectory
    (plugin_dir / "snapshots").mkdir(exist_ok=True)

    # Add the plugin to `rules/mod.rs`.
    with (ROOT_DIR / "src/rules/mod.rs").open("a") as fp:
        fp.write(f"pub mod {dir_name(plugin)};")

    # Add the relevant sections to `src/registry.rs`.
    content = (ROOT_DIR / "src/registry.rs").read_text()

    with (ROOT_DIR / "src/registry.rs").open("w") as fp:
        for line in content.splitlines():
            indent = get_indent(line)

            if line.strip() == "// ruff":
                fp.write(f"{indent}// {plugin}")
                fp.write("\n")

            elif line.strip() == "/// Ruff-specific rules":
                fp.write(f"{indent}/// [{plugin}]({url})\n")
                fp.write(f'{indent}#[prefix = "{prefix_code}"]\n')
                fp.write(f"{indent}{pascal_case(plugin)},")
                fp.write("\n")

            fp.write(line)
            fp.write("\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate boilerplate for a new Flake8 plugin.",
        epilog=(
            "Example usage: python scripts/add_plugin.py flake8-pie "
            "--url https://pypi.org/project/flake8-pie/"
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
    parser.add_argument(
        "--prefix",
        required=False,
        default="TODO",
        type=str,
        help="Prefix code for the plugin. Leave empty to manually fill.",
    )
    args = parser.parse_args()

    main(plugin=args.plugin, url=args.url, prefix_code=args.prefix)
