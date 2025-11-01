import bar
import foo
import os


def foo():
    pass

import last

import late  # comment

from late_paren1 import (  # comment
    value
)

from late_paren2 import (
    value  # comment
)

from late_paren3 import (
    value
)  # comment

from late_paren4 import (
    value1,
    value2,  # comment
    value3,
)
