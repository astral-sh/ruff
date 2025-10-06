# ensure trailing comments are preserved
import x  # comment
from x import a  # comment
from x import a, b  # comment
from x import a as b  # comment
from x import a as b, b as c  # comment

from x import (
    a,  # comment
)
from x import (
    a,  # comment
    b,
)

# ensure comma is added
from x import (
    a  # comment
)

# follow black style by merging cases without own-line comments
from x import (
    a  # alpha
    ,  # beta
    b,
)

# ensure intermixed comments are all preserved
from x import (  # one
    # two
    a  # three
    # four
    ,  # five
    # six
)  # seven

from x import (  # alpha
    # bravo
    a  # charlie
    # delta
    as  # echo
    # foxtrot
    b  # golf
    # hotel
    ,  # india
    # juliet
)  # kilo
