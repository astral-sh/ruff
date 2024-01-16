"""Test: ensure that we visit type definitions and lambdas recursively."""

from __future__ import annotations

import re
import os
from typing import cast

cast(lambda: re.match, 1)

cast(lambda: cast(lambda: os.path, 1), 1)
