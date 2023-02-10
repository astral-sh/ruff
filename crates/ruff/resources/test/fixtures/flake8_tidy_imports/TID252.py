# ok
import other
import other.example
from other import example

# error
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

# detected, no autofix: too many levels up
from ..... import ultragrantparent
from ...... import ultragrantparent
from ....... import ultragrantparent
from ......... import ultragrantparent
from ........................... import ultragrantparent
from .....parent import ultragrantparent
from .........parent import ultragrantparent
from ...........................parent import ultragrantparent
