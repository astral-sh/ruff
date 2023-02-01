"""Generate an MkDocs-compatible `docs` and `mkdocs.yml` from the README.md."""
import argparse
import shutil
from pathlib import Path

import yaml

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

FATHOM_SCRIPT: str = (
    '<script src="https://cdn.usefathom.com/script.js" data-site="DUAEBFLB" defer>'
    "</script>"
)


def main() -> None:
    """Generate an MkDocs-compatible `docs` and `mkdocs.yml`."""
    with Path("README.md").open(encoding="utf8") as fp:
        content = fp.read()

    # Remove the documentation link, since we're _in_ the docs.
    if DOCUMENTATION_LINK not in content:
        msg = "README.md is not in the expected format."
        raise ValueError(msg)
    content = content.replace(DOCUMENTATION_LINK, "")

    Path("docs").mkdir(parents=True, exist_ok=True)

    # Split the README.md into sections.
    for title, filename in SECTIONS:
        with Path(f"docs/{filename}").open("w+") as f:
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
    shutil.copy("CONTRIBUTING.md", "docs/contributing.md")

    # Add the nav section to mkdocs.yml.
    with Path("mkdocs.template.yml").open(encoding="utf8") as fp:
        config = yaml.safe_load(fp)
    config["nav"] = [
        {"Overview": "index.md"},
        {"Installation and Usage": "installation-and-usage.md"},
        {"Configuration": "configuration.md"},
        {"Rules": "rules.md"},
        {"Settings": "settings.md"},
        {"Editor Integrations": "editor-integrations.md"},
        {"FAQ": "faq.md"},
        {"Contributing": "contributing.md"},
    ]
    config["extra"] = {"analytics": {"provider": "fathom"}}

    Path(".overrides/partials/integrations/analytics").mkdir(parents=True, exist_ok=True)
    with Path(".overrides/partials/integrations/analytics/fathom.html").open("w+") as fp:
        fp.write(FATHOM_SCRIPT)

    with Path("mkdocs.yml").open("w+") as fp:
        yaml.safe_dump(config, fp)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generate an MkDocs-compatible `docs` and `mkdocs.yml`.",
    )
    args = parser.parse_args()
    main()
