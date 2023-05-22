"""Update ruff.json in schemastore.

This script will clone astral-sh/schemastore, update the schema and push the changes
to a new branch tagged with the ruff git hash. You should see a URL to create the PR
to schemastore in the CLI. The script is separated into three blocks so you can rerun
the schema generation in the middle before committing.
"""

# %%
import json
from pathlib import Path
from subprocess import check_call, check_output

schemastore_fork = "https://github.com/astral-sh/schemastore"
schemastore_upstream = "https://github.com/SchemaStore/schemastore"
ruff_repo = "https://github.com/charliermarsh/ruff"

root = Path(check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
schemastore = root.joinpath("schemastore")
ruff_json = Path("src/schemas/json/ruff.json")

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

# %%

schema = json.loads(Path("ruff.schema.json").read_text())
schema["$id"] = "https://json.schemastore.org/ruff.json"
schemastore.joinpath(ruff_json).write_text(
    json.dumps(dict(sorted(schema.items())), indent=2, ensure_ascii=False),
)
check_call(["prettier", "--write", ruff_json], cwd=schemastore)

# %%

# https://stackoverflow.com/a/9393642/3549270
if check_output(["git", "status", "-s"]).strip():
    commit_url = f"{ruff_repo}/commit/{current_sha}"
    commit_body = f"This updates ruff's JSON schema to [{current_sha}]({commit_url})"
    # https://stackoverflow.com/a/22909204/3549270
    check_call(
        ["git", "commit", "-a", "-m", "Update ruff's JSON schema", "-m", commit_body],
        cwd=schemastore,
    )
    check_call(
        ["git", "push", "--set-upstream", "origin", branch],
        cwd=schemastore,
    )
else:
    print("No changes")
