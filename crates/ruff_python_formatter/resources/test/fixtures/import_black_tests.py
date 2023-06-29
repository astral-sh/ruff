from __future__ import annotations

import argparse
import glob
import os
import pathlib


def import_fixture(black_input: pathlib.Path, fixture_set: str, fixture_name: str):
    output_directory = pathlib.Path(os.path.dirname(__file__)).joinpath("black").joinpath(fixture_set)

    output_directory.mkdir(parents=True, exist_ok=True)

    fixture_path = output_directory.joinpath(fixture_name)
    expect_path = fixture_path.with_suffix(".py.expect")

    with (
        black_input.open("r", encoding="UTF8") as black_file,
        fixture_path.open("w", encoding="UTF8") as fixture_file,
        expect_path.open("w", encoding="UTF8") as expect_file
    ):
        lines = black_file.readlines()
        expected = []
        input = []

        for (i, line) in enumerate(lines):
            if line.rstrip() == "# output":
                expected = lines[i + 1:]
                break
            else:
                input.append(line)

        if not expected:
            # If there's no output marker, tread the whole file as already pre-formatted
            expected = input

        fixture_file.write("".join(input).strip() + "\n")
        expect_file.write("".join(expected).strip() + "\n")


FIXTURE_SETS = [
    "py_36",
    "py_37",
    "py_38",
    "py_39",
    "py_310",
    "py_311",
    "simple_cases",
    "miscellaneous",
    "",
    "type_comments"
]

IGNORE_LIST = [
    "pep_572_remove_parens.py", # Reformatting bugs
    "pep_646.py", # Rust Python parser bug

    # Contain syntax errors
    "async_as_identifier.py",
    "invalid_header.py",
    "pattern_matching_invalid.py",

    # Python 2
    "python2_detection.py"
]


def import_fixtures(black_dir: str):
    test_directory = pathlib.Path(black_dir, "tests/data")

    if not test_directory.exists():
        print(
            "Black directory does not contain a 'tests/data' directory. Does the directory point to a full black "
            "checkout (git clone https://github.com/psf/black.git)?")
        return

    for fixture_set in FIXTURE_SETS:
        fixture_directory = test_directory.joinpath(fixture_set)
        fixtures = glob.glob("*.py", root_dir=fixture_directory)

        if not fixtures:
            print(f"Fixture set '{fixture_set}' contains no python files")
            return

        for fixture in fixtures:
            if fixture in IGNORE_LIST:
                print(f"Ignoring fixture '{fixture_set}:{fixture}")
                continue

            print(f"Importing fixture '{fixture_set}:{fixture}")
            import_fixture(fixture_directory.joinpath(fixture), fixture_set, fixture)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(
        description="Imports the test suite from black.",
        epilog="import_black_tests.py <path_to_black_repository>"
    )

    parser.add_argument("black_dir", type=pathlib.Path)

    args = parser.parse_args()

    black_dir = args.black_dir

    import_fixtures(black_dir)
