import os
import re
from pathlib import Path

ROOT_DIR = Path(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def dir_name(origin: str) -> str:
    return origin.replace("-", "_")


def pascal_case(origin: str) -> str:
    """Convert from snake-case to PascalCase."""
    return "".join(word.title() for word in origin.split("-"))


def get_indent(line: str) -> str:
    return re.match(r"^\s*", line).group()  # pyright: ignore[reportOptionalMemberAccess]
