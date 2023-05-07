#!/usr/bin/env python3
"""Check code snippets in docs are formatted by black."""
import argparse
import os
import re
import textwrap
from collections.abc import Sequence
from pathlib import Path
from re import Match

import black
from black.mode import Mode, TargetVersion
from black.parsing import InvalidInput

TARGET_VERSIONS = ["py37", "py38", "py39", "py310", "py311"]
SNIPPED_RE = re.compile(
    r"(?P<before>^(?P<indent> *)```\s*python\n)"
    r"(?P<code>.*?)"
    r"(?P<after>^(?P=indent)```\s*$)",
    re.DOTALL | re.MULTILINE,
)


class CodeBlockError(Exception):
    """A code block parse error."""


def format_str(
    src: str,
    black_mode: black.FileMode,
) -> tuple[str, Sequence[CodeBlockError]]:
    """Format a single docs file string."""
    errors: list[CodeBlockError] = []

    def _snipped_match(match: Match[str]) -> str:
        code = textwrap.dedent(match["code"])
        try:
            code = black.format_str(code, mode=black_mode)
        except InvalidInput as e:
            errors.append(CodeBlockError(e))

        code = textwrap.indent(code, match["indent"])
        return f'{match["before"]}{code}{match["after"]}'

    src = SNIPPED_RE.sub(_snipped_match, src)
    return src, errors


def format_file(
    file: Path,
    black_mode: black.FileMode,
    error_known: bool,  # noqa: FBT001
    args: argparse.Namespace,
) -> int:
    """Check the formatting of a single docs file."""
    with file.open() as f:
        contents = f.read()

    # Remove everything before the first example
    contents = contents[contents.find("## Example") :]

    # Remove everything after the last example
    contents = contents[: contents.rfind("```")] + "```"

    new_contents, errors = format_str(contents, black_mode)

    if errors and not args.skip_errors and not error_known:
        for error in errors:
            rule_name = file.name.split(".")[0]
            print(f"Docs parse error for {rule_name} docs: {error}")

        return 2

    if contents != new_contents:
        rule_name = file.name.split(".")[0]
        print(
            f"Rule {rule_name} docs are not formatted. This section should be"
            " rewritten to:",
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


def main(argv: Sequence[str] | None = None) -> int:
    """Check code snippets in docs are formatted by black."""
    parser = argparse.ArgumentParser(
        description="Check code snippets in docs are formatted by black.",
    )
    parser.add_argument("--skip-errors", action="store_true")
    parser.add_argument("--generate-docs", action="store_true")
    args = parser.parse_args(argv)

    if args.generate_docs:
        # Generate docs
        from generate_mkdocs import main as generate_docs

        generate_docs()

    # Get static docs
    static_docs = []
    for file in os.listdir("docs"):
        if file.endswith(".md"):
            static_docs.append(Path("docs") / file)

    # Check rules generated
    if not Path("docs/rules").exists():
        print("Please generate rules first.")
        return 1

    # Get generated rules
    generated_docs = []
    for file in os.listdir("docs/rules"):
        if file.endswith(".md"):
            generated_docs.append(Path("docs/rules") / file)

    if len(generated_docs) == 0:
        print("Please generate rules first.")
        return 1

    black_mode = Mode(
        target_versions={TargetVersion[val.upper()] for val in TARGET_VERSIONS},
    )

    # For some docs, we don't want black to fix the formatting as this would remove the
    # reason for the example. These files will be stored in
    # `scripts/known_formatting_violations.txt`

    with Path("scripts/known_rule_formatting_violations.txt").open() as f:
        known_formatting_violations = f.read().splitlines()

    # For some docs, black is unable to parse the example code. These files will be
    # stored at `scripts/known_rule_parse_errors.txt`

    with Path("scripts/known_rule_parse_errors.txt").open() as f:
        known_parse_errors = f.read().splitlines()

    # Check known formatting violations and parse errors are sorted alphabetically and
    # have no duplicates. This will reduce the diff when adding new violations

    for known_list, file_string in [
        (known_formatting_violations, "formatting violations"),
        (known_parse_errors, "parse errors"),
    ]:
        if known_list != sorted(known_list):
            print(
                f"Known {file_string} is not sorted alphabetically. Please sort and"
                " re-run.",
            )
            return 1

        if len(known_list) != len(set(known_list)):
            print(f"Known {file_string} has duplicates. Please remove them and re-run.")
            return 1

    violations = 0
    errors = 0
    for file in [*static_docs, *generated_docs]:
        rule_name = file.name.split(".")[0]
        if rule_name in known_formatting_violations:
            continue

        error_known = rule_name in known_parse_errors

        result = format_file(file, black_mode, error_known, args)
        if result == 1:
            violations += 1
        elif result == 2 and not error_known:
            errors += 1

    if violations > 0:
        print(f"Formatting violations identified: {violations}")

    if errors > 0:
        print(f"New code block parse errors identified: {errors}")

    if violations > 0 or errors > 0:
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
