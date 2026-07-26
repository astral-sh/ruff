# Membership tests are not exempt from E402 (see `E402_6.py`), because mypy does not narrow on
# them: after `assert sys.platform in ("win32", "cygwin")`, the rest of the module stays reachable
# on every platform.
#
# This lives in its own fixture because the end of the import block is sticky: once a statement
# ends it, every later import is flagged regardless of what that statement was.

import sys

assert sys.platform in ("win32", "cygwin")

import msvcrt
