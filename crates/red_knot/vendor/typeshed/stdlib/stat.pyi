import sys
from _stat import *
from typing import Literal

if sys.version_info >= (3, 13):
    # https://github.com/python/cpython/issues/114081#issuecomment-2119017790
    SF_RESTRICTED: Literal[0x00080000]
