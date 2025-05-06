#!/usr/bin/env python3
from __future__ import annotations

import os
import re

files_to_process = [
    "crates/ty_python_semantic/resources/mdtest/narrow/issubclass.md",
    "crates/ty_python_semantic/resources/mdtest/narrow/type.md",
    "crates/ty_python_semantic/resources/mdtest/subscript/class.md",
    "crates/ty_python_semantic/resources/mdtest/stubs/class.md",
    "crates/ty_python_semantic/resources/mdtest/subscript/tuple.md",
    "crates/ty_python_semantic/resources/mdtest/type_of/typing_dot_Type.md",
    "crates/ty_python_semantic/resources/mdtest/type_of/basic.md",
    "crates/ty_python_semantic/resources/mdtest/unary/custom.md",
    "crates/ty_python_semantic/resources/mdtest/type_api.md",
    "crates/ty_python_semantic/resources/mdtest/import/star.md",
]


def process_file(file_path):
    if not os.path.exists(file_path):
        print(f"File not found: {file_path}")
        return

    with open(file_path) as file:
        content = file.read()

    # First, handle simple class literals like "revealed: Literal[Class]"
    pattern = r"revealed: Literal\[([A-Za-z0-9_]+)\]"
    content = re.sub(pattern, r"revealed: <class '\1'>", content)

    # Handle "error:" message with class references
    pattern = r'(error: [^"]+"[^"]*?)Literal\[([A-Za-z0-9_]+)\]([^"]*?")'
    content = re.sub(pattern, r"\1<class \'\2\'>\3", content)

    # Handle multi-class literals like "revealed: Literal[X, Y]"
    # This is trickier because we need to handle commas properly
    pattern = r"revealed: Literal\[([A-Za-z0-9_, ]+)\]"

    def replace_multi_class(match):
        classes = match.group(1).split(", ")
        result = []
        for cls in classes:
            if (
                cls in ["True", "False", "None"]
                or cls.isdigit()
                or cls.startswith('"')
                or cls.startswith("'")
            ):
                result.append(cls)
            else:
                result.append(f"<class '{cls}'>")
        return f'revealed: Literal[{", ".join(result)}]'

    content = re.sub(pattern, replace_multi_class, content)

    # Handle tuples with class literals
    pattern = r"revealed: tuple\[(.*?)\]"

    def replace_tuple_items(match):
        items = match.group(1).split(", ")
        result = []
        for item in items:
            if item.startswith("Literal[") and "]" in item:
                # Extract the class name from Literal[X]
                class_name = item[len("Literal[") : -1]
                if (
                    class_name not in ["True", "False", "None"]
                    and not class_name.isdigit()
                    and not class_name.startswith('"')
                    and not class_name.startswith("'")
                ):
                    result.append(f"<class '{class_name}'>")
                else:
                    result.append(item)
            else:
                result.append(item)
        return f'revealed: tuple[{", ".join(result)}]'

    content = re.sub(pattern, replace_tuple_items, content)

    with open(file_path, "w") as file:
        file.write(content)

    print(f"Processed: {file_path}")


for file_path in files_to_process:
    process_file(file_path)
