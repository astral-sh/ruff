#!/usr/bin/python3

from __future__ import annotations

import argparse
from pathlib import Path


def import_fixture(fixture: Path, fixture_set: str):
    """
    Imports a single fixture by writing the input and expected output to the black directory.
    """

    output_directory = Path(__file__).parent.joinpath("black").joinpath(fixture_set)
    output_directory.mkdir(parents=True, exist_ok=True)

    fixture_path = output_directory.joinpath(fixture.name)
    expect_path = fixture_path.with_suffix(".py.expect")

    with (
        fixture.open("r") as black_file,
        fixture_path.open("w") as fixture_file,
        expect_path.open("w") as expect_file
    ):
        lines = iter(black_file)
        expected = []
        input = []

        for line in lines:
            if line.rstrip() == "# output":
                expected = list(lines)
                break
            else:
                input.append(line)

        if not expected:
            # If there's no output marker, tread the whole file as already pre-formatted
            expected = input

        fixture_file.write("".join(input).strip() + "\n")
        expect_file.write("".join(expected).strip() + "\n")


# The name of the folders in the `data` for which the tests should be imported
FIXTURE_SETS = [
    "fast",
    "py_36",
    "py_37",
    "py_38",
    "py_39",
    "py_310",
    "py_311",
    "py_312",
    "simple_cases",
    "miscellaneous",
    ".",
    "type_comments"
]

# Tests that ruff doesn't fully support yet and, therefore, should not be imported
IGNORE_LIST = [
    # Contain syntax errors
    "async_as_identifier.py",
    "invalid_header.py",
    "pattern_matching_invalid.py",

    # Python 2
    "python2_detection.py"
]


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
