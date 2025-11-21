# Errors.

if 1 in set([1]):
    print("Single-element set")

if 1 in set((1,)):
    print("Single-element set")

if 1 in set({1}):
    print("Single-element set")

if 1 in frozenset([1]):
    print("Single-element set")

if 1 in frozenset((1,)):
    print("Single-element set")

if 1 in frozenset({1}):
    print("Single-element set")

if 1 in set(set([1])):
    print('Recursive solution')



# Non-errors.

if 1 in set((1, 2)):
    pass

if 1 in set([1, 2]):
    pass

if 1 in set({1, 2}):
    pass

if 1 in frozenset((1, 2)):
    pass

if 1 in frozenset([1, 2]):
    pass

if 1 in frozenset({1, 2}):
    pass

if 1 in set(1,):
    pass

if 1 in set(1,2):
    pass

if 1 in set((x for x in range(2))):
    pass

# https://github.com/astral-sh/ruff/issues/20255
import math

# set() and frozenset() with NaN
if math.nan in set([math.nan]):
    print("This should be marked unsafe")

if math.nan in frozenset([math.nan]):
    print("This should be marked unsafe")
