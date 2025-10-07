# Changelog

## 0.14.0

Released on 2025-10-07.

### Breaking changes

- Update default and latest Python versions for 3.14 ([#20725](https://github.com/astral-sh/ruff/pull/20725))

### Preview features

- \[`flake8-bugbear`\] Include certain guaranteed-mutable expressions: tuples, generators, and assignment expressions (`B006`) ([#20024](https://github.com/astral-sh/ruff/pull/20024))
- \[`refurb`\] Add fixes for `FURB101` and `FURB103` ([#20520](https://github.com/astral-sh/ruff/pull/20520))
- \[`ruff`\] Extend `FA102` with listed PEP 585-compatible APIs ([#20659](https://github.com/astral-sh/ruff/pull/20659))

### Bug fixes

- \[`flake8-annotations`\] Fix return type annotations to handle shadowed builtin symbols (`ANN201`, `ANN202`, `ANN204`, `ANN205`, `ANN206`) ([#20612](https://github.com/astral-sh/ruff/pull/20612))
- \[`flynt`\] Fix f-string quoting for mixed quote joiners (`FLY002`) ([#20662](https://github.com/astral-sh/ruff/pull/20662))
- \[`isort`\] Fix inserting required imports before future imports (`I002`) ([#20676](https://github.com/astral-sh/ruff/pull/20676))
- \[`ruff`\] Handle argfile expansion errors gracefully ([#20691](https://github.com/astral-sh/ruff/pull/20691))
- \[`ruff`\] Skip `RUF051` if `else`/`elif` block is present ([#20705](https://github.com/astral-sh/ruff/pull/20705))
- \[`ruff`\] Improve handling of intermixed comments inside from-imports ([#20561](https://github.com/astral-sh/ruff/pull/20561))

### Documentation

- \[`flake8-comprehensions`\] Clarify fix safety documentation (`C413`) ([#20640](https://github.com/astral-sh/ruff/pull/20640))

### Contributors

- [@danparizher](https://github.com/danparizher)
- [@terror](https://github.com/terror)
- [@TaKO8Ki](https://github.com/TaKO8Ki)
- [@ntBre](https://github.com/ntBre)
- [@njhearp](https://github.com/njhearp)
- [@amyreese](https://github.com/amyreese)
- [@IDrokin117](https://github.com/IDrokin117)
- [@chirizxc](https://github.com/chirizxc)

## 0.13.x

See [changelogs/0.13.x](./changelogs/0.13.x.md)

## 0.12.x

See [changelogs/0.12.x](./changelogs/0.12.x.md)

## 0.11.x

See [changelogs/0.11.x](./changelogs/0.11.x.md)

## 0.10.x

See [changelogs/0.10.x](./changelogs/0.10.x.md)

## 0.9.x

See [changelogs/0.9.x](./changelogs/0.9.x.md)

## 0.8.x

See [changelogs/0.8.x](./changelogs/0.8.x.md)

## 0.7.x

See [changelogs/0.7.x](./changelogs/0.7.x.md)

## 0.6.x

See [changelogs/0.6.x](./changelogs/0.6.x.md)

## 0.5.x

See [changelogs/0.5.x](./changelogs/0.5.x.md)

## 0.4.x

See [changelogs/0.4.x](./changelogs/0.4.x.md)

## 0.3.x

See [changelogs/0.3.x](./changelogs/0.3.x.md)

## 0.2.x

See [changelogs/0.2.x](./changelogs/0.2.x.md)

## 0.1.x

See [changelogs/0.1.x](./changelogs/0.1.x.md)
