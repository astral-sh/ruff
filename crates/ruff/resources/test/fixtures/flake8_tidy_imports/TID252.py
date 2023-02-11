# OK
import other
import other.example
from other import example

# TID252
from . import sibling
from .sibling import example
from .. import parent
from ..parent import example
from ... import grandparent
from ...grandparent import example
from  .parent import hello
from .\
    parent import \
        hello_world
from \
    ..parent\
    import \
    world_hello

# TID252 (without autofix; too many levels up)
from ..... import ultragrantparent
from ...... import ultragrantparent
from ....... import ultragrantparent
from ......... import ultragrantparent
from ........................... import ultragrantparent
from .....parent import ultragrantparent
from .........parent import ultragrantparent
from ...........................parent import ultragrantparent
