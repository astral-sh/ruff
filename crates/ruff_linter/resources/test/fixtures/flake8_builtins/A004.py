import some as sum
import float
from some import other as int
from some import input, exec
from directory import new as dir

# See: https://github.com/astral-sh/ruff/issues/13037
import sys

if sys.version_info < (3, 11):
    from exceptiongroup import BaseExceptionGroup, ExceptionGroup
