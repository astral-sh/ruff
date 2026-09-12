# Migrating to Categories

_This feature currently requires [preview](preview.md) mode._

Ruff's new categories are intended to make it easier to select large numbers of rules that you're
interested in, without wading through lists of opaque linter prefixes. See [Rule
Categories](linter.md#rule-categories) for a full description of each category, but a quick summary
of the kinds of code flagged by rules in each category will be helpful here too:

- `correctness`: code that is incorrect
- `suspicious`: code that is likely incorrect but may be intentional
- `complexity`: code that can be written in a simpler way
- `performance`: code that can be written in a more efficient way
- `style`: code that can be written in a more idiomatic way
- `security`: code that may cause security vulnerabilities
- `formatting`: code that should be formatted in a different way
- `pedantic`: code that some people would disagree with
- `restriction`: code that uses basic language features

These are arranged in roughly descending order of severity, with `correctness` lints catching the
most severe issues with the highest confidence and `restriction` lints preventing you from using
basic features of the language like `assert` (`S101`) or `print` (`T201`). In line with that
hierarchy, the first five categories (`correctness`, `suspicious`, `complexity`, `performance`, and
`style`) are enabled by default. See [Rules by Category](rules-by-category.md) for a full list of
rules in each category.

## Rules of thumb

We expect virtually all projects to want the `correctness` rules enabled. The lints in this category
include syntax errors that are not yet mapped to `invalid-syntax` diagnostics and other problems
that cause immediate runtime errors. From there, `suspicious` is likely to be the next most helpful
category. It includes rules that flag deprecated code, as well as classic footguns like
`mutable-argument-default` (`B006`) that are almost always wrong but may be intentional in some
cases. We tried to be conservative with the rules in `correctness`, so many rules like this that are
only _usually_ accurate are found in `suspicious` instead. In general, you should feel comfortable
using a `ruff: ignore` comment on diagnostics from the `suspicious` or lower categories but think
twice (or share feedback!) about suppressing a `correctness` lint.

The rules in the `complexity`, `performance`, and `style` categories are all stylistic, but we feel
that these rules represent widely-accepted styles in the Python community. As demonstrated by their
inclusion in the defaults, we expect most projects to want these rules enabled. Again, even if you
enable these categories, you should feel comfortable ignoring certain rules project-wide or inline
with suppression comments.

The `security` rules are focused on issues that may cause security vulnerabilities and overlap
closely with the `flake8-bandit` (`S`) linter group. They are in their own category because these
rules are intentionally biased toward false positives over false negatives and can be quite noisy.
However, if your project is security-critical or just security-conscious, you will likely want to
enable this entire category.

The `formatting` category contains rules that overlap with code formatters like the Ruff formatter
or Black. If you use a code formatter, you will likely want to leave this category off. On the other
hand, if you don't use a code formatter and rely on lint rules to format your code, this category is
for you. In the future, this category should allow us to stabilize the formatting-related
`pycodestyle` (`E`) rules that otherwise didn't fit well into the `E` linter group.

`pedantic` rules, as you may guess, are pedantic, which can mean either "noisy," leading to many
diagnostics, or overly opinionated, suggesting changes that many Python users disagree with. Unlike
the `security` and `formatting` categories, you probably will not want to enable this category as a
whole. Instead, we intend for rules from the `pedantic` category to be selected individually. This
also goes for `restriction`, which contains even more restrictive rules.

## Analyzing your configuration

It's not quite a one-liner, but once you've upgraded to a Ruff version that supports categories, you
can generate a report on your current usage of each category with a command like the following
(assuming you're using a shell like bash or zsh and have [jq](https://jqlang.org/) installed):

```console
 $ jq -sr '
  INDEX(.[0][]; .) as $enabled
  | .[1]
  | group_by(.category)[]
  | .[0].category as $category
  | length as $total
  | map(select($enabled[.name]))
  | length as $count
  | "\($category) \($count) (\(100 * $count / $total | round)%)"
' <(ruff check --show-settings | awk '/^linter\.rules\.enabled/,/]/ { if (/^\t/) print $1 }' | jq -Rn '[inputs]') \
  <(ruff rule --all --output-format=json)
```

This produces output like:

```text
correctness 74 (56%)
suspicious 75 (44%)
complexity 67 (56%)
performance 2 (29%)
style 40 (58%)
security 4 (5%)
formatting 4 (6%)
pedantic 43 (14%)
restriction 3 (21%)
```

with each category, followed by the number of rules selected in that category, along with the
percentage of the category that represents. Somewhat unsurprisingly, this suggests that we should
probably expand our selection to include the rest of the `correctness` lints, as well as the other
default categories, from which we generally select most rules already.

## Trying the categories and sharing feedback

If you'd like to try out the new categories without replacing your current configuration, the
defaults, or a smaller subset like `correctness` and `suspicious`, are a great place to start. You
can append them to an existing `select` configuration, or add them with `extend-select`.

However, if you'd like to try the full category experience, we recommend replacing your
configuration with the new default selection:

=== "pyproject.toml"

    ```toml
    [tool.ruff.lint]
    preview = true
    select = [
        "correctness",
        "suspicious",
        "complexity",
        "performance",
        "style",
    ]
    ```

=== "ruff.toml"

    ```toml
    [lint]
    preview = true
    select = [
        "correctness",
        "suspicious",
        "complexity",
        "performance",
        "style",
    ]
    ```

and selecting or ignoring additional rules until you reproduce your current selection. We eventually
hope to deprecate and remove the legacy linter groups, so trying to reach a comparable rule
selection without using any linter groups is the best way to preview that. We're open to iterating
on the specific rules in each of the new categories, to adding new top-level categories, and to
introducing non-linter secondary groups to make this selection process easier, so please share any
feedback in the [tracking issue](https://github.com/astral-sh/ruff/issues/27959).

### Migration script

An exact migration can be quite verbose because linter groups don't map closely to the new
categories. However, if you want to preserve your current rule selection precisely, the script below
may be a good start. It `select`s all of the categories needed to cover your current rule selection
and then `ignore`s any extra rules to give you a matching set. It also groups the `ignore` entries
by category so you can experiment with removing `correctness` ignores first, for example.

??? note "Simple migration script"

    ```py
    # /// script
    # requires-python = ">=3.13"
    # dependencies = []
    #
    # [tool.uv]
    # no-build = true
    # exclude-newer = "P7D"
    # ///

    from __future__ import annotations

    import argparse
    import functools
    import json
    import operator
    import re
    import subprocess
    from textwrap import indent

    SETTINGS_RE = re.compile(r"^linter\.rules\.enabled = \[([^]]+)\]", flags=re.MULTILINE)
    CATEGORY_ORDER = [
        "correctness",
        "suspicious",
        "complexity",
        "performance",
        "style",
        "security",
        "formatting",
        "pedantic",
        "restriction",
    ]

    category_sort_key = functools.partial(operator.indexOf, CATEGORY_ORDER)


    def run_command(args: list[str]) -> str:
        return subprocess.run(args, check=True, text=True, capture_output=True).stdout


    def enabled_rules(ruff: str) -> set[str]:
        output = run_command([ruff, "check", "--show-settings"])
        enabled_rules = SETTINGS_RE.search(output)
        if enabled_rules is None:
            raise ValueError("Expected enabled rules")

        return {line.split()[0] for line in enabled_rules[1].splitlines() if line}


    def rule_categories(ruff: str) -> dict[str, str]:
        output = json.loads(run_command([ruff, "rule", "--all", "--output-format=json"]))
        return {
            rule["name"]: rule["category"]
            for rule in output
            if "Removed" not in rule["status"] and "Deprecated" not in rule["status"]
        }


    def main():
        parser = argparse.ArgumentParser()
        parser.add_argument(
            "--ruff", default="ruff", help="The path to the Ruff executable to use"
        )

        args = parser.parse_args()

        current_rules = enabled_rules(args.ruff)
        rule_to_category = rule_categories(args.ruff)

        enabled_categories = {rule_to_category[rule] for rule in current_rules}

        categories = indent(
            ",\n".join(
                f'"{category}"'
                for category in sorted(enabled_categories, key=category_sort_key)
            ),
            "    ",
        )
        print(f"select = [\n{categories},\n]")

        new_rules = {
            rule
            for rule, category in rule_to_category.items()
            if category in enabled_categories
        }
        to_ignore = new_rules - current_rules
        print("ignore = [")
        for category in CATEGORY_ORDER:
            rules = sorted(
                rule for rule in to_ignore if rule_to_category[rule] == category
            )
            if rules:
                print(f"    # {category}")
                for rule in rules:
                    print(f'    "{rule}",')
        print("]")


    if __name__ == "__main__":
        main()
    ```
