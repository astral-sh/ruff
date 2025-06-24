"""The rule should trigger here because adding the __future__ import would
allow TC003 to apply."""

import collections

def func(c: collections.Counter) -> int:
    return len(c)