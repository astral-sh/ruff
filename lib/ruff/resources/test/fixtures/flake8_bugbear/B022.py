"""
Should emit:
B022 - on lines 8
"""

import contextlib
from contextlib import suppress

with contextlib.suppress():
    raise ValueError

with suppress():
    raise ValueError

with contextlib.suppress(ValueError):
    raise ValueError

exceptions_to_suppress = []
if True:
    exceptions_to_suppress.append(ValueError)

with contextlib.suppress(*exceptions_to_suppress):
    raise
