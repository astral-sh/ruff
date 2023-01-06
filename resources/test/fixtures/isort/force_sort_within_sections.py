from a import a1  # import_from
from c import * # import_from_star
import a  # import
import c.d
import b as b1  # import_as

from ..parent import *
from .my import fn
from . import my
from .my.nested import fn2
from ...grandparent import fn3
