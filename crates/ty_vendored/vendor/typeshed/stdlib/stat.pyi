"""Constants/functions for interpreting results of os.stat() and os.lstat().

Suggested usage: from stat import *
"""

import sys
from _stat import *
from typing import Final

if sys.version_info >= (3, 13):
    # https://github.com/python/cpython/issues/114081#issuecomment-2119017790
    SF_RESTRICTED: Final = 0x00080000
