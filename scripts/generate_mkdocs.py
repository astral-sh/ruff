"""Generate an MkDocs-compatible `docs` and `mkdocs.yml` from the README.md."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
from pathlib import Path
from typing import NamedTuple

import mdformat
import yaml

from _mdformat_utils import add_no_escape_text_plugin


class Section(NamedTuple):
    """A section to include in the MkDocs documentation."""

    title: str
    filename: str
    generated: bool


SECTIONS: list[Section] = [
    Section("Overview", "index.md", generated=True),
    Section("Tutorial", "tutorial.md", generated=False),
    Section("Installing Ruff", "installation.md", generated=False),
    Section("The Ruff Linter", "linter.md", generated=False),
    Section("The Ruff Formatter", "formatter.md", generated=False),
    Section("Configuring Ruff", "configuration.md", generated=False),
    Section("Preview", "preview.md", generated=False),
    Section("Rules", "rules.md", generated=True),
    Section("Settings", "settings.md", generated=True),
    Section("Versioning", "versioning.md", generated=False),
    Section("Integrations", "integrations.md", generated=False),
    Section("FAQ", "faq.md", generated=False),
    Section("Contributing", "contributing.md", generated=True),
]


LINK_REWRITES: dict[str, str] = {
    "https://docs.astral.sh/ruff/": "index.md",
    "https://docs.astral.sh/ruff/configuration/": "configuration.md",
    "https://docs.astral.sh/ruff/configuration/#pyprojecttoml-discovery": (
        "configuration.md#pyprojecttoml-discovery"
    ),
    "https://docs.astral.sh/ruff/contributing/": "contributing.md",
    "https://docs.astral.sh/ruff/integrations/": "integrations.md",
    "https://docs.astral.sh/ruff/faq/#how-does-ruff-compare-to-flake8": (
        "faq.md#how-does-ruff-compare-to-flake8"
    ),
    "https://docs.astral.sh/ruff/installation/": "installation.md",
    "https://docs.astral.sh/ruff/rules/": "rules.md",
    "https://docs.astral.sh/ruff/settings/": "settings.md",
    "#whos-using-ruff": "https://github.com/astral-sh/ruff#whos-using-ruff",
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
        before = content
        after = content.replace(f"({src})", f"({dst})")
        if before == after:
            msg = f"Unexpected link rewrite in README.md: {src}"
            raise ValueError(msg)
        content = after

    if m := re.search(r"\(https://docs.astral.sh/ruff/.*\)", content):
        msg = f"Unexpected absolute link to documentation: {m.group(0)}"
        raise ValueError(msg)

    Path("docs").mkdir(parents=True, exist_ok=True)

    # Split the README.md into sections.
    for title, filename, generated in SECTIONS:
        if not generated:
            continue

        with Path(f"docs/{filename}").open("w+", encoding="utf8") as f:
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

    # Format rules docs
    add_no_escape_text_plugin()
    for rule_doc in Path("docs/rules").glob("*.md"):
        mdformat.file(rule_doc, extensions=["mkdocs", "admonition", "no-escape-text"])

    with Path("mkdocs.template.yml").open(encoding="utf8") as fp:
        config = yaml.safe_load(fp)

    # Add the redirect section to mkdocs.yml.
    rules = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "run",
                "-p",
                "ruff",
                "--",
                "rule",
                "--all",
                "--output-format",
                "json",
            ],
        ),
    )
    config["plugins"].append(
        {
            "redirects": {
                "redirect_maps": {
                    f'rules/{rule["code"]}.md': f'rules/{rule["name"]}.md'
                    for rule in rules
                },
            },
        },
    )

    # Add the nav section to mkdocs.yml.
    config["nav"] = [{section.title: section.filename} for section in SECTIONS]

    with Path("mkdocs.generated.yml").open("w+", encoding="utf8") as fp:
        yaml.safe_dump(config, fp)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate an MkDocs-compatible `docs` and `mkdocs.yml`.",
    )
    args = parser.parse_args()
    main()
