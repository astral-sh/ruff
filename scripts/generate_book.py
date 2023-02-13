"""Generate the Markdown files for mdbook in `docs/src`."""
import argparse
import shutil
import subprocess
from pathlib import Path

SECTIONS: list[tuple[str, str]] = [
    ("Overview", "index.md"),
    ("Installation and Usage", "installation-and-usage.md"),
    ("Configuration", "configuration.md"),
    ("Rules", "rules.md"),
    ("Settings", "settings.md"),
    ("Editor Integrations", "editor-integrations.md"),
    ("FAQ", "faq.md"),
]

DOCUMENTATION_LINK: str = (
    "This README is also available as [documentation](https://beta.ruff.rs/docs/)."
)


def main() -> None:
    """Generate the Markdown files for mdbook in `docs/src`."""

    subprocess.run(["cargo", "dev", "generate-docs"], check=True)

    with Path("README.md").open(encoding="utf8") as fp:
        content = fp.read()

    # Remove the documentation link, since we're _in_ the docs.
    if DOCUMENTATION_LINK not in content:
        msg = "README.md is not in the expected format."
        raise ValueError(msg)
    content = content.replace(DOCUMENTATION_LINK, "")

    # Replace all GitHub links with relative links.
    content = content.replace(
        "https://github.com/charliermarsh/ruff/blob/main/docs/rules/",
        "rules/",
    )

    docs_path = Path("docs/src")
    docs_path.mkdir(parents=True, exist_ok=True)

    with (docs_path.parent / "SUMMARY.md").open("r") as f:
        text = f.read()

    rules_item = "- [Rules](./rules.md)\n"
    assert rules_item in text
    rule_items = (f"    - [{f.stem}](rules/{f.name})\n" for f in sorted(docs_path.joinpath("rules").iterdir()))
    text = text.replace(rules_item, rules_item + "".join(rule_items))

    with (docs_path / "SUMMARY.md").open("w") as f:
        f.write(text)

    # Split the README.md into sections.
    for title, filename in SECTIONS:
        with docs_path.joinpath(filename).open("w+") as f:
            f.write(f"# {title}\n")
            block = content.split(f"<!-- Begin section: {title} -->")
            if len(block) != 2:
                msg = f"Section {title} not found in README.md"
                raise ValueError(msg)

            block = block[1].split(f"<!-- End section: {title} -->")
            if len(block) != 2:
                msg = f"Section {title} not found in README.md"
                raise ValueError(msg)

            f.write(block[0])

    # Copy the CONTRIBUTING.md.
    shutil.copy("CONTRIBUTING.md", docs_path / "contributing.md")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description=__doc__,
    )
    args = parser.parse_args()
    main()
