# Regression test for https://github.com/astral-sh/ruff/issues/8211

# fmt: off
from dataclasses import dataclass

if True:
    if False:
        x: int # Optional[int]
