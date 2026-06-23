# /// script
# requires-python = ">=3.13"
# dependencies = []
# ///

from __future__ import annotations

import json
import pathlib
import subprocess

GENERATED_HEADER = "<!-- This file is generated. DO NOT EDIT -->"

RUFF_TEMPLATE = """{GENERATED_HEADER}

# Ruff

Ruff is an extremely fast Python linter and code formatter.

See the [documentation](https://docs.astral.sh/ruff/) or
[repository](https://github.com/astral-sh/ruff) for more information.

This crate is the entry point to the Ruff command-line interface. The Rust API exposed here is not
considered public interface.

This is version {ruff_version}. The source can be found [here]({source_url}).

The following Ruff workspace members are also available:

{WORKSPACE_MEMBERS}

Ruff's workspace members are considered internal and will have frequent breaking changes.

See Ruff's [crate versioning policy](https://docs.astral.sh/ruff/versioning/#crate-versioning) for
details on versioning.
"""


MEMBER_TEMPLATE = """{GENERATED_HEADER}

# {name}

This crate is an internal component of [Ruff](https://crates.io/crates/ruff). The Rust API exposed
here is unstable and will have frequent breaking changes.

This version ({crate_version}) is a component of [Ruff {ruff_version}]({ruff_crates_io_url}). The
source can be found [here]({source_url}).

See Ruff's [crate versioning policy](https://docs.astral.sh/ruff/versioning/#crate-versioning) for
details on versioning.
"""


REPO_URL = "https://github.com/astral-sh/ruff"
PRETTIER_VERSION = "3.8.3"


def main() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    )
    content = json.loads(result.stdout)
    packages = {package["id"]: package for package in content["packages"]}

    # Find the Ruff version from the ruff crate.
    ruff_version = None
    for package in content["packages"]:
        if package["name"] == "ruff":
            ruff_version = package["version"]
            break
    if ruff_version is None:
        raise RuntimeError("Could not find ruff crate")

    workspace_root = pathlib.Path(content["workspace_root"])
    readme_path = workspace_root / "crates" / "ruff" / "README.md"

    workspace_members = []
    for workspace_member in content["workspace_members"]:
        package = packages[workspace_member]
        name = package["name"]
        # Skip the main Ruff crate.
        if name == "ruff":
            continue
        # Skip crates with publish = false.
        if package.get("publish") == []:
            continue
        workspace_members.append(name)

    workspace_members.sort()

    members_list = "\n".join(
        f"- [{name}](https://crates.io/crates/{name})" for name in workspace_members
    )

    # Generate the README for the main Ruff crate.
    ruff_source_url = f"{REPO_URL}/blob/{ruff_version}/crates/ruff"
    readme_content = RUFF_TEMPLATE.format(
        GENERATED_HEADER=GENERATED_HEADER,
        WORKSPACE_MEMBERS=members_list,
        ruff_version=ruff_version,
        source_url=ruff_source_url,
    )
    readme_path.write_text(readme_content)

    # Track all generated README paths for formatting at the end.
    generated_paths = [readme_path]

    # Generate READMEs for all workspace members.
    for workspace_member in content["workspace_members"]:
        package = packages[workspace_member]
        name = package["name"]

        # Skip the main Ruff crate (already handled above).
        if name == "ruff":
            continue
        # Skip crates that aren't released to crates.io.
        if package.get("publish") == []:
            continue

        manifest_path = pathlib.Path(package["manifest_path"])
        crate_dir = manifest_path.parent
        member_readme_path = crate_dir / "README.md"

        # Preserve hand-written crate READMEs.
        if member_readme_path.exists():
            existing_content = member_readme_path.read_text()
            if not existing_content.startswith(GENERATED_HEADER):
                print(f"Skipping {name}: existing README without generated header")
                continue

        crate_version = package["version"]
        relative_crate_path = crate_dir.relative_to(workspace_root)
        source_url = f"{REPO_URL}/blob/{ruff_version}/{relative_crate_path}"

        ruff_crates_io_url = f"https://crates.io/crates/ruff/{ruff_version}"
        member_readme_content = MEMBER_TEMPLATE.format(
            GENERATED_HEADER=GENERATED_HEADER,
            name=name,
            crate_version=crate_version,
            ruff_version=ruff_version,
            ruff_crates_io_url=ruff_crates_io_url,
            source_url=source_url,
        )
        member_readme_path.write_text(member_readme_content)
        generated_paths.append(member_readme_path)

        print(f"Generated README for {name}")

    # Format all generated READMEs once at the end.
    subprocess.run(
        ["npx", "--yes", f"prettier@{PRETTIER_VERSION}", "--write"]
        + [str(path) for path in generated_paths],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


if __name__ == "__main__":
    main()
