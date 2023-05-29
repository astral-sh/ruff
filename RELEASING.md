# Creating a new release

This is a guide how to create a new ruff release

1. Update the version with `rg 0.0.269 --files-with-matches | xargs sed -i 's/0.0.269/0.0.270/g'`
1. Update [BREAKING_CHANGES.md](BREAKING_CHANGES.md)
1. Create a PR with the version and BREAKING_CHANGES.md updated
1. Merge the PR
1. Run the release workflow with the version number (without starting `v`) as input. Make sure
   main has your merged PR as last commit
1. The release workflow will do the following:
   1. Build all the assets. If this fails (even though we tested in step 4), we haven’t tagged or
      uploaded anything, you can restart after pushing a fix
   1. Upload to pypi
   1. Create and push the git tag (from pyproject.toml). We create the git tag only here
      because we can't change it ([#4468](https://github.com/charliermarsh/ruff/issues/4468)), so
      we want to make sure everything up to and including publishing to pypi worked.
   1. Attach artifacts to draft GitHub release
   1. Trigger downstream repositories. This can fail without causing fallout, it is possible (if
      inconvenient) to trigger the downstream jobs manually
1. Create release notes in GitHub UI and promote from draft to proper release(<https://github.com/charliermarsh/ruff/releases/new>)
1. If needed, [update the schemastore](https://github.com/charliermarsh/ruff/blob/main/scripts/update_schemastore.py)
1. If needed, update ruff-lsp and ruff-vscode
