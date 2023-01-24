import argparse
from pathlib import Path

EMPTY_LINE = "# EMPTY LINE WITH WHITESPACE (this comment will be removed)"


def read_data_from_file(file_name: Path) -> tuple[str, str]:
    with open(file_name, "r", encoding="utf8") as test:
        lines = test.readlines()
    _input: list[str] = []
    _output: list[str] = []
    result = _input
    for line in lines:
        line = line.replace(EMPTY_LINE, "")
        if line.rstrip() == "# output":
            result = _output
            continue

        result.append(line)
    if _input and not _output:
        # If there's no output marker, treat the entire file as already pre-formatted.
        _output = _input[:]
    return "".join(_input).strip() + "\n", "".join(_output).strip() + "\n"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("file", type=Path)
    args = parser.parse_args()

    file_name: Path = args.file
    (input_content, output_content) = read_data_from_file(file_name)

    with open(file_name, "r", encoding="utf8") as fp1:
        with open(file_name.with_suffix(".bak"), "w+", encoding="utf8") as fp2:
            fp2.write(fp1.read())

    with open(file_name, "w+", encoding="utf8") as fp:
        fp.write(input_content)

    relative_name = str(
        file_name.relative_to("crates/ruff_fmt/resources/test/fixtures/black")
    )
    suffix = relative_name.replace("/", "__")
    snapshot_path = (
        Path("crates/ruff_fmt/src/snapshots") / f"ruff_fmt__tests__{suffix}.snap"
    )
    with open(snapshot_path, "w+", encoding="utf8") as fp:
        fp.write(
            f"""---
source: src/source_code/mod.rs
assertion_line: 0
expression: formatted
---
{output_content}
"""
        )


if __name__ == "__main__":
    main()
