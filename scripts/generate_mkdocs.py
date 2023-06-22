"""Generate an MkDocs-compatible `docs` and `mkdocs.yml` from the README.md."""
from __future__ import annotations

import argparse
import re
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
    Section("Tutorial", "tutorial.md", generated=False),
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

LINK_REWRITES: dict[str, str] = {
    "https://beta.ruff.rs/docs/": "index.md",
    "https://beta.ruff.rs/docs/configuration/": "configuration.md",
    "https://beta.ruff.rs/docs/configuration/#pyprojecttoml-discovery": (
        "configuration.md#pyprojecttoml-discovery"
    ),
    "https://beta.ruff.rs/docs/contributing/": "contributing.md",
    "https://beta.ruff.rs/docs/editor-integrations/": "editor-integrations.md",
    "https://beta.ruff.rs/docs/faq/": "faq.md",
    "https://beta.ruff.rs/docs/faq/#how-does-ruff-compare-to-flake8": (
        "faq.md#how-does-ruff-compare-to-flake8"
    ),
    "https://beta.ruff.rs/docs/installation/": "installation.md",
    "https://beta.ruff.rs/docs/rules/": "rules.md",
    "https://beta.ruff.rs/docs/rules/#error-e": "rules.md#error-e",
    "https://beta.ruff.rs/docs/settings/": "settings.md",
    "https://beta.ruff.rs/docs/usage/": "usage.md",
}


def clean_file_content(content: str, title: str) -> str:
    """Add missing title, fix the header depth, and remove trailing empty lines."""
    lines = content.splitlines()
    if lines[0].startswith("# "):
        return content

    in_code_block = False
    for i, line in enumerate(lines):
        if line.startswith("```"):
            in_code_block = not in_code_block
        if not in_code_block and line.startswith("#"):
            lines[i] = line[1:]

    # Remove trailing empty lines.
    for line in reversed(lines):
        if line == "":
            del lines[-1]
        else:
            break

    content = "\n".join(lines) + "\n"

    # Add a missing title.
    return f"# {title}\n\n" + content


def main() -> None:
    """Generate an MkDocs-compatible `docs` and `mkdocs.yml`."""
    subprocess.run(["cargo", "dev", "generate-docs"], check=True)

    with Path("README.md").open(encoding="utf8") as fp:
        content = fp.read()

    # Rewrite links to the documentation.
    for src, dst in LINK_REWRITES.items():
        content = content.replace(f"({src})", f"({dst})")
    if m := re.search(r"\(https://beta.ruff.rs/docs/.*\)", content):
        msg = f"Unexpected absolute link to documentation: {m.group(0)}"
        raise ValueError(msg)

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
                file_content = subprocess.check_output(
                    ["cargo", "dev", "generate-options"],
                    encoding="utf-8",
                )
            else:
                block = content.split(f"<!-- Begin section: {title} -->\n\n")
                if len(block) != 2:
                    msg = f"Section {title} not found in README.md"
                    raise ValueError(msg)

                block = block[1].split(f"\n<!-- End section: {title} -->")
                if len(block) != 2:
                    msg = f"Section {title} not found in README.md"
                    raise ValueError(msg)

                file_content = block[0]

                if filename == "rules.md":
                    file_content += "\n" + subprocess.check_output(
                        ["cargo", "dev", "generate-rules-table"],
                        encoding="utf-8",
                    )

            f.write(clean_file_content(file_content, title))

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
