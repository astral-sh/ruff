# A tuple of prefixes is not exempt from E402 (see `E402_6.py`), because mypy narrows on
# `sys.platform.startswith(...)` only when it is passed a single string literal.
#
# This lives in its own fixture because the end of the import block is sticky: once a statement
# ends it, every later import is flagged regardless of what that statement was.

import sys

assert sys.platform.startswith(("win32", "cygwin"))

import msvcrt
