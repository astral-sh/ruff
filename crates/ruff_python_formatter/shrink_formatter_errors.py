"""Take `format-dev --stability-check` output and shrink all stability errors into a
single Python file. Used to update https://github.com/astral-sh/ruff/issues/5828 ."""

from __future__ import annotations

import json
import os
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from subprocess import check_output
from tempfile import NamedTemporaryFile

from tqdm import tqdm

root = Path(
    check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip(),
)
target = root.joinpath("target")

error_report = target.joinpath("formatter-ecosystem-errors.txt")
error_lines_prefix = "Unstable formatting "


def get_filenames() -> list[str]:
    files = []
    for line in error_report.read_text().splitlines():
        if not line.startswith(error_lines_prefix):
            continue
        files.append(line.removeprefix(error_lines_prefix))
    return files


def shrink_file(file: str) -> tuple[str, str]:
    """Returns filename and minimization"""
    with NamedTemporaryFile(suffix=".py") as temp_file:
        print(f"Starting {file}")
        ruff_dev = target.joinpath("release").joinpath("ruff_dev")
        check_output(
            [
                target.joinpath("release").joinpath("ruff_shrinking"),
                file,
                temp_file.name,
                "Unstable formatting",
                f"{ruff_dev} format-dev --stability-check {temp_file.name}",
            ],
        )
        print(f"Finished {file}")
        return file, Path(temp_file.name).read_text()


def main():
    storage = target.joinpath("minimizations.json")
    output_file = target.joinpath("minimizations.py")
    if storage.is_file():
        outputs = json.loads(storage.read_text())
    else:
        outputs = {}
    files = sorted(set(get_filenames()) - set(outputs))
    # Each process will saturate one core
    with ThreadPoolExecutor(max_workers=os.cpu_count()) as executor:
        tasks = [executor.submit(shrink_file, file) for file in files]
        for future in tqdm(as_completed(tasks), total=len(files)):
            file, output = future.result()
            outputs[file] = output
            storage.write_text(json.dumps(outputs, indent=4))

    # Write to one shareable python file
    with output_file.open("w") as formatted:
        for file, code in sorted(json.loads(storage.read_text()).items()):
            file = file.split("/target/checkouts/")[1]
            formatted.write(f"# {file}\n{code}\n")


if __name__ == "__main__":
    main()
