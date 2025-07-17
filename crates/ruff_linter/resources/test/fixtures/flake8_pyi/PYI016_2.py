# This is a regression test for https://github.com/astral-sh/ruff/issues/19403
from typing import Union
isinstance(None, Union[None, None])