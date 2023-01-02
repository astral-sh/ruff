import argparse
import os

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def dir_name(plugin: str) -> str:
    return plugin.replace("-", "_")


def main(*, plugin: str) -> None:
    # Create the test fixture folder.
    os.makedirs(
        os.path.join(ROOT_DIR, f"resources/tests/fixtures/{dir_name(plugin)}"),
        exist_ok=True,
    )

    # Create the Rust module.
    os.makedirs(os.path.join(ROOT_DIR, f"src/{dir_name(plugin)}"), exist_ok=True)
    with open(os.path.join(ROOT_DIR, f"src/{dir_name(plugin)}/mod.rs"), "w") as fp:
        fp.write("pub mod checks;\n")
        fp.write("pub mod plugins;\n")
        fp.write("pub mod settings;\n")
        fp.write("\n")
        fp.write(
            """#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    // #[test_case(CheckCode::A001, Path::new("A001_0.py"); "A001_0")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/pygrep-hooks")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
"""
        )

    # Add the plugin to `lib.rs`.
    with open(os.path.join(ROOT_DIR, "src/lib.rs"), "a") as fp:
        fp.write(f"pub mod {dir_name(plugin)};")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate all boilerplate for a new plugin."
    )
    parser.add_argument("plugin", type=str, help="The name of the plugin to generate.")
    args = parser.parse_args()

    main(plugin=args.plugin)
