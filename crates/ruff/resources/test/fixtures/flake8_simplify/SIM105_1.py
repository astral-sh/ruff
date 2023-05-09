"""Case: There's a random import, so it should add `contextlib` after it."""
import math

try:
    math.sqrt(-1)
except ValueError:  # SIM105
    pass
