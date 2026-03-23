"""Tests for TC009 - TYPE_CHECKING block before imports."""

# Error: TYPE_CHECKING block before regular imports
import logging
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # TC009
    from collections.abc import Sequence

from mylib import MyClass

# OK: TYPE_CHECKING block after all imports
import os
from typing import TYPE_CHECKING as TC

from another_lib import AnotherClass

if TC:
    from collections.abc import Mapping


# OK: no imports after TYPE_CHECKING block
import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Set
