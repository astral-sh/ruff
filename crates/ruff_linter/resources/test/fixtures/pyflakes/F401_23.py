"""Test: ensure that we treat strings in `typing.Annotation` as type definitions."""

from pathlib import Path
from re import RegexFlag
from typing import Annotated

p: Annotated["Path", int] = 1
