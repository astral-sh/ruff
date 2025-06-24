"""The rule should trigger here because adding the __future__ import would
allow TC002 to apply."""

import pandas

def func(df: pandas.DataFrame) -> int:
    return len(df)
