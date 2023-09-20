"""Case: There's a random import, so it should add `contextlib` after it."""
import math

# SIM105
try:
    math.sqrt(-1)
except ValueError:
    pass
