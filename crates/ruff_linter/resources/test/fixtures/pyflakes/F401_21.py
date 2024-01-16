from __future__ import annotations

from collections import Counter
from typing import cast


def get_most_common_fn(k):
    return lambda c: cast(Counter, c).most_common(k)
