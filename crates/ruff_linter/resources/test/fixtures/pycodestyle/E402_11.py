# Reaching `sys.platform` through an alias is not exempt from E402 (see `E402_6.py`), because mypy
# matches the name `sys` as it is written rather than resolving it to the module.
#
# This lives in its own fixture because the end of the import block is sticky: once a statement
# ends it, every later import is flagged regardless of what that statement was.

import sys as system

assert system.platform == "win32"

import msvcrt
