"""Test case: strings used within calls within type annotations."""

from typing import Callable

import bpy
from mypy_extensions import VarArg

class LightShow(bpy.types.Operator):
    label = "Create Character"
    name = "lightshow.letter_creation"

    filepath: bpy.props.StringProperty(subtype="FILE_PATH")  # OK


def f(x: Callable[[VarArg("os")], None]):  # F821
    pass
