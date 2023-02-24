"""Generate an MkDocs-compatible `docs` and `mkdocs.yml` from the README.md."""
import argparse
import shutil
import subprocess
from pathlib import Path
from typing import NamedTuple

import yaml


class Section(NamedTuple):
    """A section to include in the MkDocs documentation."""

    title: str
    filename: str
    generated: bool


SECTIONS: list[Section] = [
    Section("Overview", "index.md", generated=True),
    Section("Installation", "installation.md", generated=False),
    Section("Usage", "usage.md", generated=False),
    Section("Configuration", "configuration.md", generated=False),
    Section("Rules", "rules.md", generated=True),
    Section("Settings", "settings.md", generated=True),
    Section("Editor Integrations", "editor-integrations.md", generated=False),
    Section("FAQ", "faq.md", generated=False),
    Section("Contributing", "contributing.md", generated=True),
]

FATHOM_SCRIPT: str = (
    '<script src="https://cdn.usefathom.com/script.js" data-site="DUAEBFLB" defer>'
    "</script>"
)


def main() -> None:
    """Generate an MkDocs-compatible `docs` and `mkdocs.yml`."""

    subprocess.run(["cargo", "dev", "generate-docs"], check=True)

    with Path("README.md").open(encoding="utf8") as fp:
        content = fp.read()

    # Convert any inter-documentation links to relative links.
    content = content.replace("https://beta.ruff.rs", "")

    Path("docs").mkdir(parents=True, exist_ok=True)

    # Split the README.md into sections.
    for title, filename, generated in SECTIONS:
        if not generated:
            continue

        with Path(f"docs/{filename}").open("w+") as f:
            if filename == "contributing.md":
                # Copy the CONTRIBUTING.md.
                shutil.copy("CONTRIBUTING.md", "docs/contributing.md")
                continue

            if filename == "settings.md":
                f.write(
                    subprocess.check_output(
                        ["cargo", "dev", "generate-options"],
                        encoding="utf-8",
                    ),
                )
                continue

            block = content.split(f"<!-- Begin section: {title} -->")
            if len(block) != 2:
                msg = f"Section {title} not found in README.md"
                raise ValueError(msg)

            block = block[1].split(f"<!-- End section: {title} -->")
            if len(block) != 2:
                msg = f"Section {title} not found in README.md"
                raise ValueError(msg)

            f.write(block[0])

            if filename == "rules.md":
                f.write(
                    subprocess.check_output(
                        ["cargo", "dev", "generate-rules-table"],
                        encoding="utf-8",
                    ),
                )

    # Add the nav section to mkdocs.yml.
    with Path("mkdocs.template.yml").open(encoding="utf8") as fp:
        config = yaml.safe_load(fp)
    config["nav"] = [{section.title: section.filename} for section in SECTIONS]
    config["extra"] = {"analytics": {"provider": "fathom"}}

    Path(".overrides/partials/integrations/analytics").mkdir(
        parents=True,
        exist_ok=True,
    )
    with Path(".overrides/partials/integrations/analytics/fathom.html").open(
        "w+",
    ) as fp:
        fp.write(FATHOM_SCRIPT)

    with Path("mkdocs.yml").open("w+") as fp:
        yaml.safe_dump(config, fp)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate an MkDocs-compatible `docs` and `mkdocs.yml`.",
    )
    args = parser.parse_args()
    main()
