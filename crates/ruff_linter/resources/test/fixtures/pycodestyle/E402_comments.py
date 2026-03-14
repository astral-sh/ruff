import os

print("something")

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


# Prevent F403
os, last, late, value, value1, value2, value3
