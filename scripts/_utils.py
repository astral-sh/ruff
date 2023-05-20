import re
from pathlib import Path
from typing import Callable

ROOT_DIR = Path(__file__).resolve().parent.parent


def dir_name(linter_name: str) -> str:
    return linter_name.replace("-", "_").split(" ")[0]


def pascal_case(linter_name: str) -> str:
    """Convert from snake-case to PascalCase."""
    return "".join(word.title() for word in linter_name.split("_")[0].split("-"))


def snake_case(name: str) -> str:
    """Convert from PascalCase to snake_case."""
    return "".join(
        f"_{word.lower()}" if word.isupper() else word for word in name
    ).lstrip("_")


def get_indent(line: str) -> str:
    return re.match(r"^\s*", line).group()  # type: ignore[union-attr]


def key_test_case(nb_digit: int) -> Callable[[str], tuple[str, int, str, int]]:
    def key(line: str) -> tuple[str, int, str, int]:
        *_, (rule, prefix, code, subcode) = re.findall(
            r'Rule::(.*?), Path::new\((?:"([A-Z]+)([0-9]+)?|.*?)(_[0-9]+)?(?:.pyi?)"',
            line,
        )
        subcode = int(subcode[1:]) if subcode else -1
        prefix = prefix or ""
        code = int(code + "0" * (nb_digit - len(code))) if code is not None else -1
        return prefix, code, rule, subcode

    return key


def key_pub_use(line: str) -> str:
    return line.replace("(crate)", "")


def key_mod(line: str) -> str:
    return line.replace("pub(crate) ", "")


def key_code_to_rule_pair(line: str) -> str:
    return line.lstrip().replace("// Reserved: ", "")
