#!/usr/bin/python3

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path


def import_fixture(fixture: Path, fixture_set: str):
    """
    Imports a single fixture by writing the input and expected output to the black directory.
    """

    output_directory = Path(__file__).parent.joinpath("black").joinpath(fixture_set)
    output_directory.mkdir(parents=True, exist_ok=True)

    with fixture.open("r") as black_file:
        lines = iter(black_file)
        expected = []
        input = []
        flags = None

        for line in lines:
            if line.rstrip() == "# output":
                expected = list(lines)
                break
            elif not input and line.startswith("# flags:"):
                flags = line
            else:
                input.append(line)

        if not expected:
            # If there's no output marker, treat the whole file as already pre-formatted
            expected = input

        black_options = {}
        extension = "py"

        if flags:
            if "--preview" in flags or "--unstable" in flags:
                black_options["preview"] = "enabled"

            if "--pyi" in flags:
                extension = "pyi"

            if "--line-ranges=" in flags:
                # Black preserves the flags for line-ranges tests to not mess up the line numbers
                input.insert(0, flags)

            if "--line-length=" in flags:
                [_, length_and_rest] = flags.split("--line-length=", 1)
                length = length_and_rest.split(" ", 1)[0]
                length = int(length)
                black_options["line_width"] = 1 if length == 0 else length

            if "--minimum-version=" in flags:
                [_, version] = flags.split("--minimum-version=", 1)
                version = version.split(" ", 1)[0]
                black_options["target_version"] = version.strip()

            if "--skip-magic-trailing-comma" in flags:
                black_options["magic_trailing_comma"] = "ignore"

        fixture_path = output_directory.joinpath(fixture.name).with_suffix(f".{extension}")
        expect_path = fixture_path.with_suffix(f".{extension}.expect")
        options_path = fixture_path.with_suffix(".options.json")

        options = OPTIONS_OVERRIDES.get(fixture.name, black_options)

        if len(options) > 0:
            if extension == "pyi":
                options["source_type"] = "Stub"

            with options_path.open("w") as options_file:
                json.dump(options, options_file)
        elif os.path.exists(options_path):
            os.remove(options_path)

        with (
            fixture_path.open("w") as fixture_file,
            expect_path.open("w") as expect_file
        ):
            fixture_file.write("".join(input).strip() + "\n")
            expect_file.write("".join(expected).strip() + "\n")


# The name of the folders in the `data` for which the tests should be imported
FIXTURE_SETS = [
    "cases",
    "miscellaneous",
]

# Tests that ruff doesn't fully support yet and, therefore, should not be imported
IGNORE_LIST = [
    # Contain syntax errors
    "async_as_identifier.py",
    "invalid_header.py",
    "pattern_matching_invalid.py",
    "pep_572_do_not_remove_parens.py",

    # Python 2
    "python2_detection.py",

    # Uses a different output format
    "decorators.py",

    # Tests line ranges that fall outside the source range. This is a CLI test case and not a formatting test case.
    "line_ranges_outside_source.py",
]

# Specs for which to override the formatter options
OPTIONS_OVERRIDES = {
    "context_managers_38.py": {
        "target_version": "py38"
    },
    "context_managers_autodetect_38.py" : {
        "target_version": "py38"
    }
}


def import_fixtures(black_dir: str):
    """Imports all the black fixtures"""

    test_directory = Path(black_dir, "tests/data")

    if not test_directory.exists():
        print(
            "Black directory does not contain a 'tests/data' directory. Does the directory point to a full black "
            "checkout (git clone https://github.com/psf/black.git)?")
        return

    for fixture_set in FIXTURE_SETS:
        fixture_directory = test_directory.joinpath(fixture_set)
        fixtures = fixture_directory.glob("*.py")

        if not fixtures:
            print(f"Fixture set '{fixture_set}' contains no python files")
            return

        for fixture in fixtures:
            if fixture.name in IGNORE_LIST:
                print(f"Ignoring fixture '{fixture}")
                continue

            print(f"Importing fixture '{fixture}")
            import_fixture(fixture, fixture_set)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(
        description="Imports the test suite from black.",
        epilog="import_black_tests.py <path_to_black_repository>"
    )

    parser.add_argument("black_dir", type=Path)

    args = parser.parse_args()

    black_dir = args.black_dir

    import_fixtures(black_dir)
