"""Test: referencing an import via TypeAlias (with future annotations)."""
from __future__ import annotations

import sys

import numpy as np

if sys.version_info >= (3, 10):
    from typing import TypeAlias
else:
    from typing_extensions import TypeAlias


CustomInt: TypeAlias = np.int8 | np.int16
