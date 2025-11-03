"""Generate an MkDocs-compatible `docs` and `mkdocs.yml` from the README.md."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
from collections.abc import Sequence
from itertools import chain
from pathlib import Path
from typing import NamedTuple

import mdformat
import yaml


class Section(NamedTuple):
    """A section to include in the MkDocs documentation."""

    title: str
    filename: str
    generated: bool
    # If subsections is present, the `filename` and `generated` value is unused.
    subsections: Sequence[Section] | None = None


SECTIONS: list[Section] = [
    Section("Overview", "index.md", generated=True),
    Section("Tutorial", "tutorial.md", generated=False),
    Section("Installing Ruff", "installation.md", generated=False),
    Section("The Ruff Linter", "linter.md", generated=False),
    Section("The Ruff Formatter", "formatter.md", generated=False),
    Section(
        "Editors",
        "",
        generated=False,
        subsections=[
            Section("Editor Integration", "editors/index.md", generated=False),
            Section("Setup", "editors/setup.md", generated=False),
            Section("Features", "editors/features.md", generated=False),
            Section("Settings", "editors/settings.md", generated=False),
            Section("Migrating from ruff-lsp", "editors/migration.md", generated=False),
        ],
    ),
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
    "https://docs.astral.sh/ruff/configuration/#config-file-discovery": (
        "configuration.md#config-file-discovery"
    ),
    "https://docs.astral.sh/ruff/contributing/": "contributing.md",
    "https://docs.astral.sh/ruff/editors": "editors/index.md",
    "https://docs.astral.sh/ruff/editors/setup": "editors/setup.md",
    "https://docs.astral.sh/ruff/faq/#how-does-ruffs-linter-compare-to-flake8": (
        "faq.md#how-does-ruffs-linter-compare-to-flake8"
    ),
    "https://docs.astral.sh/ruff/faq/#how-does-ruffs-formatter-compare-to-black": (
        "faq.md#how-does-ruffs-formatter-compare-to-black"
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


def generate_rule_metadata(rule_doc: Path) -> None:
    """Add frontmatter metadata containing a rule's code and description.

    For example:
    ```yaml
    ---
    description: Checks for abstract classes without abstract methods or properties.
    tags:
    - B024
    ---
    ```
    """
    # Read the rule doc into lines.
    with rule_doc.open("r", encoding="utf-8") as f:
        lines = f.readlines()

    # Get the description and rule code from the rule doc lines.
    rule_code = None
    description = None
    what_it_does_found = False
    for line in lines:
        if line == "\n":
            continue

        # Assume that the only first-level heading is the rule title and code.
        #
        # For example: given `# abstract-base-class-without-abstract-method (B024)`,
        # extract the rule code (`B024`).
        if line.startswith("# "):
            rule_code = line.strip().rsplit("(", 1)
            rule_code = rule_code[1][:-1]

        if line.startswith("## What it does"):
            what_it_does_found = True
            continue  # Skip the '## What it does' line

        if what_it_does_found and not description:
            description = line.removesuffix("\n")

        if all([rule_code, description]):
            break
    else:
        if not rule_code:
            raise ValueError("Missing title line")

        if not what_it_does_found:
            raise ValueError(f"Missing '## What it does' in {rule_doc}")

    with rule_doc.open("w", encoding="utf-8") as f:
        f.writelines(
            "\n".join(
                [
                    "---",
                    "description: |-",
                    f"  {description}",
                    "tags:",
                    f"- {rule_code}",
                    "---",
                    "",
                    "",
                ]
            )
        )
        f.writelines(lines)


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
    for title, filename, generated, _ in SECTIONS:
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

    for rule_doc in Path("docs/rules").glob("*.md"):
        # Format rules docs. This has to be completed before adding the meta description
        # otherwise the meta description will be formatted in a way that mkdocs does not
        # support.
        mdformat.file(rule_doc, extensions=["mkdocs"])

        generate_rule_metadata(rule_doc)

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
                "redirect_maps": dict(
                    chain.from_iterable(
                        [
                            (f"rules/{rule['code']}.md", f"rules/{rule['name']}.md"),
                            (
                                f"rules/{rule['code'].lower()}.md",
                                f"rules/{rule['name']}.md",
                            ),
                        ]
                        for rule in rules
                    )
                ),
            },
        },
    )

    # Add the nav section to mkdocs.yml.
    config["nav"] = []
    for section in SECTIONS:
        if section.subsections is None:
            config["nav"].append({section.title: section.filename})
        else:
            config["nav"].append(
                {
                    section.title: [
                        {subsection.title: subsection.filename}
                        for subsection in section.subsections
                    ]
                }
            )

    with Path("mkdocs.generated.yml").open("w+", encoding="utf8") as fp:
        yaml.safe_dump(config, fp)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate an MkDocs-compatible `docs` and `mkdocs.yml`.",
    )
    args = parser.parse_args()
    main()
