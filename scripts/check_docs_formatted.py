#!/usr/bin/env python3
"""Check code snippets in docs are formatted by Ruff."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import textwrap
from pathlib import Path
from re import Match
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from collections.abc import Sequence

SNIPPED_RE = re.compile(
    r"(?P<before>^(?P<indent>\x20*)```(?:\s*(?P<language>\w+))?\n)"
    r"(?P<code>.*?)"
    r"(?P<after>^(?P=indent)```\s*$)",
    re.DOTALL | re.MULTILINE,
)

# Long explanation: https://www.rexegg.com/regex-best-trick.html
#
# Short explanation:
# Match both code blocks and shortcut links, then discard the former.
# Whatever matched by the second branch is guaranteed to never be
# part of a code block, as that would already be caught by the first.
BACKTICKED_SHORTCUT_LINK_RE = re.compile(
    rf"""(?msx)
    (?:{SNIPPED_RE}
    |  \[`(?P<name>[^`\n]+)`](?![\[(])
    )
    """
)

# For some rules, we don't want Ruff to fix the formatting as this would "fix" the
# example.
KNOWN_FORMATTING_VIOLATIONS = [
    "avoidable-escaped-quote",
    "bad-quotes-docstring",
    "bad-quotes-inline-string",
    "bad-quotes-multiline-string",
    "blank-line-after-decorator",
    "blank-line-before-class",
    "blank-line-before-function",
    "blank-line-between-methods",
    "blank-lines-after-function-or-class",
    "blank-lines-before-nested-definition",
    "blank-lines-top-level",
    "docstring-tab-indentation",
    "explicit-string-concatenation",
    "f-string-missing-placeholders",
    "incorrect-blank-line-after-class",
    "incorrect-blank-line-before-class",
    "indentation-with-invalid-multiple",
    "line-too-long",
    "missing-trailing-comma",
    "missing-whitespace",
    "missing-whitespace-after-keyword",
    "missing-whitespace-around-arithmetic-operator",
    "missing-whitespace-around-bitwise-or-shift-operator",
    "missing-whitespace-around-modulo-operator",
    "missing-whitespace-around-operator",
    "missing-whitespace-around-parameter-equals",
    "module-import-not-at-top-of-file",
    "multi-line-implicit-string-concatenation",
    "multiple-leading-hashes-for-block-comment",
    "multiple-spaces-after-comma",
    "multiple-spaces-after-keyword",
    "multiple-spaces-after-operator",
    "multiple-spaces-before-keyword",
    "multiple-spaces-before-operator",
    "multiple-statements-on-one-line-colon",
    "multiple-statements-on-one-line-semicolon",
    "no-indented-block-comment",
    "no-return-argument-annotation-in-stub",
    "no-space-after-block-comment",
    "no-space-after-inline-comment",
    "non-empty-stub-body",
    "over-indentation",
    "over-indented",
    "pass-statement-stub-body",
    "prohibited-trailing-comma",
    "redundant-backslash",
    "shebang-leading-whitespace",
    "single-line-implicit-string-concatenation",
    "surrounding-whitespace",
    "too-few-spaces-before-inline-comment",
    "too-many-blank-lines",
    "too-many-boolean-expressions",
    "trailing-comma-on-bare-tuple",
    "triple-single-quotes",
    "under-indentation",
    "unexpected-indentation-comment",
    "unexpected-spaces-around-keyword-parameter-equals",
    "unicode-kind-prefix",
    "unnecessary-class-parentheses",
    "unnecessary-escaped-quote",
    "useless-semicolon",
    "whitespace-after-decorator",
    "whitespace-after-open-bracket",
    "whitespace-before-close-bracket",
    "whitespace-before-parameters",
    "whitespace-before-punctuation",
]

# For some docs, Ruff is unable to parse the example code.
KNOWN_PARSE_ERRORS = [
    "blank-line-with-whitespace",
    "indentation-with-invalid-multiple-comment",
    "indented-form-feed",
    "missing-newline-at-end-of-file",
    "mixed-spaces-and-tabs",
    "no-indented-block",
    "non-pep695-type-alias",  # requires Python 3.12
    "syntax-error",
    "tab-after-comma",
    "tab-after-keyword",
    "tab-after-operator",
    "tab-before-keyword",
    "tab-before-operator",
    "too-many-newlines-at-end-of-file",
    "trailing-whitespace",
    "unexpected-indentation",
]


class CodeBlockError(Exception):
    """A code block parse error."""


class InvalidInput(ValueError):
    """Raised when ruff fails to parse file."""


def format_str(code: str, extension: Literal["py", "pyi"]) -> str:
    """Format a code block with ruff by writing to a temporary file."""
    # Run ruff to format the tmp file
    try:
        completed_process = subprocess.run(
            ["ruff", "format", "--stdin-filename", f"file.{extension}", "-"],
            check=True,
            capture_output=True,
            text=True,
            input=code,
        )
    except subprocess.CalledProcessError as e:
        err = e.stderr
        if "error: Failed to parse" in err:
            raise InvalidInput(err) from e

        raise NotImplementedError(
            "This error has not been handled correctly, please update "
            f"`check_docs_formatted.py\n\nError:\n\n{err}",
        ) from e

    return completed_process.stdout


def format_contents(src: str) -> tuple[str, Sequence[CodeBlockError]]:
    """Format a single docs content."""
    errors: list[CodeBlockError] = []

    def _snipped_match(match: Match[str]) -> str:
        language = match["language"]
        extension: Literal["py", "pyi"]
        match language:
            case "python":
                extension = "py"
            case "pyi":
                extension = "pyi"
            case _:
                # We are only interested in checking the formatting of py or pyi code
                # blocks so we can return early if the language is not one of these.
                return f"{match['before']}{match['code']}{match['after']}"

        code = textwrap.dedent(match["code"])
        try:
            code = format_str(code, extension)
        except InvalidInput as e:
            errors.append(CodeBlockError(e))
        except NotImplementedError as e:
            raise e

        code = textwrap.indent(code, match["indent"])
        return f"{match['before']}{code}{match['after']}"

    src = SNIPPED_RE.sub(_snipped_match, src)
    return src, errors


def format_file(file: Path, error_known: bool, args: argparse.Namespace) -> int:
    """Check the formatting of a single docs file.

    Returns the exit code for the script.
    """
    with file.open() as f:
        contents = f.read()

    if file.parent.name == "rules":
        # Check contents contains "What it does" section
        if "## What it does" not in contents:
            print(f"Docs for `{file.name}` are missing the `What it does` section.")
            return 1

        # Check contents contains "Why is this bad?" section
        if "## Why is this bad?" not in contents:
            print(f"Docs for `{file.name}` are missing the `Why is this bad?` section.")
            return 1

    # Remove everything before the first example
    contents = contents[contents.find("## Example") :]

    # Remove everything after the last example
    contents = contents[: contents.rfind("```")] + "```"

    new_contents, errors = format_contents(contents)

    if errors and not args.skip_errors and not error_known:
        for error in errors:
            rule_name = file.name.split(".")[0]
            print(
                f"Docs parse error for `{rule_name}` docs. Either fix or add to "
                f"`KNOWN_PARSE_ERRORS`. {error}",
            )

        return 2

    if contents != new_contents:
        rule_name = file.name.split(".")[0]
        print(
            f"Rule `{rule_name}` docs are not formatted. Either format the rule or add "
            f"to `KNOWN_FORMATTING_VIOLATIONS`. The example section should be "
            f"rewritten to:",
        )

        # Add indentation so that snipped can be copied directly to docs
        for line in new_contents.splitlines():
            output_line = "///"
            if len(line) > 0:
                output_line = f"{output_line} {line}"

            print(output_line)

        print("\n")

        return 1

    return 0


def find_backticked_shortcut_links(
    path: Path, all_config_names: dict[str, object]
) -> set[str]:
    """Check for links of the form: [`foobar`].

    See explanation at #16010.
    """

    with path.open() as file:
        contents = file.read()

    broken_link_names: set[str] = set()

    for match in BACKTICKED_SHORTCUT_LINK_RE.finditer(contents):
        name = match["name"]

        if name is not None and name not in all_config_names:
            broken_link_names.add(name)

    return broken_link_names


def main(argv: Sequence[str] | None = None) -> int:
    """Check code snippets in docs are formatted by Ruff."""
    parser = argparse.ArgumentParser(
        description="Check code snippets in docs are formatted by Ruff.",
    )
    parser.add_argument("--skip-errors", action="store_true")
    parser.add_argument("--generate-docs", action="store_true")
    args = parser.parse_args(argv)

    if args.generate_docs:
        # Generate docs
        from generate_mkdocs import main as generate_docs

        generate_docs()

    # Get static docs
    static_docs = [Path("docs") / f for f in os.listdir("docs") if f.endswith(".md")]

    # Check rules generated
    if not Path("docs/rules").exists():
        print("Please generate rules first.")
        return 1

    # Get generated rules
    generated_docs = [
        Path("docs/rules") / f for f in os.listdir("docs/rules") if f.endswith(".md")
    ]

    if len(generated_docs) == 0:
        print("Please generate rules first.")
        return 1

    # Check known formatting violations and parse errors are sorted alphabetically and
    # have no duplicates. This will reduce the diff when adding new violations

    for known_list, file_string in [
        (KNOWN_FORMATTING_VIOLATIONS, "formatting violations"),
        (KNOWN_PARSE_ERRORS, "parse errors"),
    ]:
        if known_list != sorted(known_list):
            print(
                f"Known {file_string} is not sorted alphabetically. Please sort and "
                f"re-run.",
            )
            return 1

        duplicates = list({x for x in known_list if known_list.count(x) > 1})
        if len(duplicates) > 0:
            print(f"Known {file_string} has duplicates:")
            print("\n".join([f"  - {x}" for x in duplicates]))
            print("Please remove them and re-run.")
            return 1

    ruff_config_output = subprocess.check_output(
        ["ruff", "config", "--output-format", "json"], encoding="utf-8"
    )
    all_config_names = json.loads(ruff_config_output)

    violations = 0
    errors = 0
    broken_links: dict[str, set[str]] = {}
    print("Checking docs formatting...")
    for file in [*static_docs, *generated_docs]:
        rule_name = file.name.split(".")[0]
        if rule_name in KNOWN_FORMATTING_VIOLATIONS:
            continue

        error_known = rule_name in KNOWN_PARSE_ERRORS

        result = format_file(file, error_known, args)
        if result == 1:
            violations += 1
        elif result == 2 and not error_known:
            errors += 1

        broken_links_in_file = find_backticked_shortcut_links(file, all_config_names)

        if broken_links_in_file:
            broken_links[file.name] = broken_links_in_file

    if violations > 0:
        print(f"Formatting violations identified: {violations}")

    if errors > 0:
        print(f"New code block parse errors identified: {errors}")

    if broken_links:
        print()
        print("Do not use backticked shortcut links: [`foobar`]")
        print(
            "They work with Mkdocs but cannot be rendered by CommonMark and GFM-compliant implementers."
        )
        print("Instead, use an explicit label:")
        print("```markdown")
        print("[`lorem.ipsum`][lorem-ipsum]")
        print()
        print("[lorem-ipsum]: https://example.com/")
        print("```")

        print()
        print("The following links are found to be broken:")

        for filename, link_names in broken_links.items():
            print(f"- {filename}:")
            print("\n".join(f"  - {name}" for name in link_names))

    if violations > 0 or errors > 0 or broken_links:
        return 1

    print("All docs are formatted correctly.")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
