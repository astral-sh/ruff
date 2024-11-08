# https://github.com/astral-sh/ruff/issues/13833

from typing import Optional


def no_default(arg: Optional): ...


def has_default(arg: Optional = None): ...


def multiple_1(arg1: Optional, arg2: Optional = None): ...


def multiple_2(arg1: Optional, arg2: Optional = None, arg3: int = None): ...


def return_type(arg: Optional = None) -> Optional: ...


def has_type_argument(arg: Optional[int] = None): ...
