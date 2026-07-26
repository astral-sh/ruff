# Comparing against anything but a string literal is not exempt from E402 (see `E402_6.py`),
# because mypy doesn't narrow when it can't read the platform name straight out of the source.
#
# This lives in its own fixture because the end of the import block is sticky: once a statement
# ends it, every later import is flagged regardless of what that statement was.

import os
import sys

assert sys.platform == os.name

import msvcrt
