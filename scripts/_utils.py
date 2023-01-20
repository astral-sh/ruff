import os
import re
from pathlib import Path

ROOT_DIR = Path(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def dir_name(linter_name: str) -> str:
    return linter_name.replace("-", "_")


def pascal_case(linter_name: str) -> str:
    """Convert from snake-case to PascalCase."""
    return "".join(word.title() for word in linter_name.split("-"))


def get_indent(line: str) -> str:
    return re.match(r"^\s*", line).group()  # pyright: ignore[reportOptionalMemberAccess]
