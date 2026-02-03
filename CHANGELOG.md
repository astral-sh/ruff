# Changelog

## 0.15.0

Released on 2026-02-03.

### Breaking changes

- Ruff 0.15 ([#22875](https://github.com/astral-sh/ruff/pull/22875))

### Preview features

- Apply formatting to markdown code blocks ([#22470](https://github.com/astral-sh/ruff/pull/22470))

### Bug fixes

- Fix suppression indentation matching ([#22903](https://github.com/astral-sh/ruff/pull/22903))

### Rule changes

- Customize where the `fix_title` sub-diagnostic appears ([#23044](https://github.com/astral-sh/ruff/pull/23044))
- \[`FastAPI`\] Add sub-diagnostic explaining why a fix was unavailable (`FAST002`) ([#22565](https://github.com/astral-sh/ruff/pull/22565))
- \[`flake8-annotations`\] Don't suggest `NoReturn` for functions raising `NotImplementedError` (`ANN201`, `ANN202`, `ANN205`, `ANN206`) ([#21311](https://github.com/astral-sh/ruff/pull/21311))
- \[`pyupgrade`\] Make fix unsafe if it deletes comments (`UP017`) ([#22873](https://github.com/astral-sh/ruff/pull/22873))
- \[`pyupgrade`\] Make fix unsafe if it deletes comments (`UP020`) ([#22872](https://github.com/astral-sh/ruff/pull/22872))
- \[`pyupgrade`\] Make fix unsafe if it deletes comments (`UP033`) ([#22871](https://github.com/astral-sh/ruff/pull/22871))
- \[`refurb`\] Do not add `abc.ABC` if already present (`FURB180`) ([#22234](https://github.com/astral-sh/ruff/pull/22234))
- \[`refurb`\] Make fix unsafe if it deletes comments (`FURB110`) ([#22768](https://github.com/astral-sh/ruff/pull/22768))
- \[`ruff`\] Add sub-diagnostics with permissions (`RUF064`) ([#22972](https://github.com/astral-sh/ruff/pull/22972))

### Server

- Identify notebooks by LSP didOpen instead of `.ipynb` file extension ([#22810](https://github.com/astral-sh/ruff/pull/22810))

### CLI

- Add `--color` cli option to force colored output ([#22806](https://github.com/astral-sh/ruff/pull/22806))

### Documentation

- Document `-` stdin convention in CLI help text ([#22817](https://github.com/astral-sh/ruff/pull/22817))
- FURB167: change example to `re.search` with `^` anchor ([#22984](https://github.com/astral-sh/ruff/pull/22984))
- Fix `TY_LOG` typo ([#22885](https://github.com/astral-sh/ruff/pull/22885))
- Fix link to Sphinx code block directives ([#23041](https://github.com/astral-sh/ruff/pull/23041))
- \[`pydocstyle`\] Clarify which quote styles are allowed (`D300`) ([#22825](https://github.com/astral-sh/ruff/pull/22825))
- [flake8_bugbear] fix: improve docs for `no_explicit_stacklevel` ([#22538](https://github.com/astral-sh/ruff/pull/22538))

### Other changes

- Update MSRV to 1.91 ([#22874](https://github.com/astral-sh/ruff/pull/22874))

### Contributors

- [@danparizher](https://github.com/danparizher)
- [@chirizxc](https://github.com/chirizxc)
- [@amyreese](https://github.com/amyreese)
- [@Jkhall81](https://github.com/Jkhall81)
- [@cwkang1998](https://github.com/cwkang1998)
- [@manzt](https://github.com/manzt)
- [@11happy](https://github.com/11happy)
- [@hugovk](https://github.com/hugovk)
- [@caiquejjx](https://github.com/caiquejjx)
- [@ntBre](https://github.com/ntBre)
- [@akawd](https://github.com/akawd)
- [@konstin](https://github.com/konstin)
