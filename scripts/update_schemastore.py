"""Update ruff.json in schemastore.

This script will clone astral-sh/schemastore, update the schema and push the changes
to a new branch tagged with the ruff git hash. You should see a URL to create the PR
to schemastore in the CLI.
"""
from __future__ import annotations

import json
from pathlib import Path
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory

schemastore_fork = "git@github.com:astral-sh/schemastore.git"
schemastore_upstream = "git@github.com:SchemaStore/schemastore.git"
ruff_repo = "https://github.com/astral-sh/ruff"
root = Path(
    check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip(),
)
ruff_json = Path("schemas/json/ruff.json")


def update_schemastore(schemastore: Path) -> None:
    if not schemastore.is_dir():
        check_call(["git", "clone", schemastore_fork, schemastore])
        check_call(
            [
                "git",
                "remote",
                "add",
                "upstream",
                schemastore_upstream,
            ],
            cwd=schemastore,
        )
    # Create a new branch tagged with the current ruff commit up to date with the latest
    # upstream schemastore
    check_call(["git", "fetch", "upstream"], cwd=schemastore)
    current_sha = check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    branch = f"update-ruff-{current_sha}"
    check_call(
        ["git", "switch", "-c", branch],
        cwd=schemastore,
    )
    check_call(
        ["git", "reset", "--hard", "upstream/master"],
        cwd=schemastore,
    )

    # Run npm install
    src = schemastore.joinpath("src")
    check_call(["npm", "install"], cwd=src)

    # Update the schema and format appropriately
    schema = json.loads(root.joinpath("ruff.schema.json").read_text())
    schema["$id"] = "https://json.schemastore.org/ruff.json"
    src.joinpath(ruff_json).write_text(
        json.dumps(dict(schema.items()), indent=2, ensure_ascii=False),
    )
    check_call(
        [
            "node_modules/.bin/prettier",
            "--plugin",
            "prettier-plugin-sort-json",
            "--write",
            ruff_json,
        ],
        cwd=src,
    )

    # Check if the schema has changed
    # https://stackoverflow.com/a/9393642/3549270
    if check_output(["git", "status", "-s"], cwd=schemastore).strip():
        # Schema has changed, commit and push
        commit_url = f"{ruff_repo}/commit/{current_sha}"
        commit_body = (
            f"This updates ruff's JSON schema to [{current_sha}]({commit_url})"
        )
        # https://stackoverflow.com/a/22909204/3549270
        check_call(
            [
                "git",
                "commit",
                "-a",
                "-m",
                "Update ruff's JSON schema",
                "-m",
                commit_body,
            ],
            cwd=schemastore,
        )
        # This should show the link to create a PR
        check_call(
            ["git", "push", "--set-upstream", "origin", branch],
            cwd=schemastore,
        )
    else:
        print("No changes")


def main() -> None:
    schemastore_existing = root.joinpath("schemastore")
    if schemastore_existing.is_dir():
        update_schemastore(schemastore_existing)
    else:
        with TemporaryDirectory() as temp_dir:
            update_schemastore(Path(temp_dir).joinpath("schemastore"))


if __name__ == "__main__":
    main()
