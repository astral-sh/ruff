#!/usr/bin/env python3

"""Demo file for testing isort float-to-top and E402 fix behaviours"""

from __future__ import annotations

import bar
import foo
import os


def foo():
    pass

import last

import late  # comment-late

from late_paren1 import (  # comment-late_paren1
    value
)

from late_paren2 import (
    value  # comment-late_paren2
)

from late_paren3 import (
    value
)  # comment-late_paren3

from late_paren4 import (
    value1,
    value2,  # comment-late_paren4
    value3,
)
