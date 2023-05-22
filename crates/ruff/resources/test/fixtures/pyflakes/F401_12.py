"""Test: module bindings are preferred over local bindings, for deferred annotations."""

from __future__ import annotations

import datetime
from typing import Optional


class Class:
    datetime: Optional[datetime.datetime]
