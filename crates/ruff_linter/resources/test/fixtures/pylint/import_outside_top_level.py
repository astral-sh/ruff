from typing import TYPE_CHECKING

# Verify that statements nested in conditionals (such as top-level type-checking blocks)
# are still considered top-level
if TYPE_CHECKING:
    import string


def import_in_function():
    import symtable  # [import-outside-toplevel]
    import os, sys  # [import-outside-toplevel]
    import time as thyme  # [import-outside-toplevel]
    import random as rand, socket as sock  # [import-outside-toplevel]
    from collections import defaultdict # [import-outside-toplevel]
    from math import sin as sign, cos as cosplay  # [import-outside-toplevel]


class ClassWithImports:
    import tokenize  # [import-outside-toplevel]

    def __init__(self):
        import trace  # [import-outside-toplevel]
