#!/usr/bin/env python3
from __future__ import annotations

import glob
import os
import re


def process_file(file_path):
    if not os.path.exists(file_path):
        print(f"File not found: {file_path}")
        return

    with open(file_path) as file:
        content = file.read()

    # Replace revealed: Literal[Class] with revealed: <class 'Class'>
    content = re.sub(
        r"revealed: Literal\[([A-Za-z0-9_]+)\]",
        lambda m: f"revealed: <class '{m.group(1)}'>",
        content,
    )

    # Replace Literal[X, Y] in error messages
    content = re.sub(
        r'(error: [^"]+"[^"]*?)Literal\[([A-Za-z0-9_, ]+)\]([^"]*?")',
        lambda m: f"{m.group(1)}<class '"
        + m.group(2).replace(", ", "', <class '")
        + f"'>{m.group(3)}",
        content,
    )

    # Replace Literal references in tuple expressions
    content = re.sub(
        r"(revealed: tuple\[[^]]*?)Literal\[([A-Za-z0-9_]+)\]([^]]*?\])",
        lambda m: f"{m.group(1)}<class '{m.group(2)}'>{m.group(3)}",
        content,
    )

    # Replace multi-class literals with proper handling of commas
    content = re.sub(
        r"revealed: Literal\[([A-Za-z0-9_, ]+)\]",
        lambda m: "revealed: Literal["
        + ", ".join(
            [
                f"<class '{c.strip()}'>"
                if c.strip() not in ["True", "False", "None"]
                and not c.strip().isdigit()
                and not c.strip().startswith('"')
                and not c.strip().startswith("'")
                else c.strip()
                for c in m.group(1).split(",")
            ]
        )
        + "]",
        content,
    )

    with open(file_path, "w") as file:
        file.write(content)

    print(f"Processed: {file_path}")


# Process all markdown files in the codebase
md_files = glob.glob(
    "crates/ty_python_semantic/resources/mdtest/**/*.md", recursive=True
)
for md_file in md_files:
    process_file(md_file)
