# Regression test for https://github.com/astral-sh/ruff/issues/8211

# fmt: off
from dataclasses import dataclass

@dataclass
class A:
    x: int # Optional[int]
