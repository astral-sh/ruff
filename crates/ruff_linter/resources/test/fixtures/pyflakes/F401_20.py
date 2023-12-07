"""Test that wildcard imports are flagged when there are no unresolved references.

Only applies to preview mode.
"""

from . import *
from foo import *
from .foo import *

x = 1
print(x + 1)
