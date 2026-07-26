# Writing the comparison the other way round is not exempt from E402 (see `E402_6.py`), because
# mypy only narrows when `sys.platform` is the left operand.
#
# This lives in its own fixture because the end of the import block is sticky: once a statement
# ends it, every later import is flagged regardless of what that statement was.

import sys

assert "win32" == sys.platform

import msvcrt
