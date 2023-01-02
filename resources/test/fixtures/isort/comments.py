# Comment 1
# Comment 2
import D

# Comment 3a
import C

# Comment 3b
import C

import B  # Comment 4

# Comment 5

# Comment 6
from A import (
    a,  # Comment 7
    b,
    c,  # Comment 8
)
from A import (
    a,  # Comment 9
    b,  # Comment 10
    c,  # Comment 11
)

from D import a_long_name_to_force_multiple_lines # Comment 12
from D import another_long_name_to_force_multiple_lines # Comment 13

from E import a  # Comment 1

from F import a  # Comment 1
from F import b
