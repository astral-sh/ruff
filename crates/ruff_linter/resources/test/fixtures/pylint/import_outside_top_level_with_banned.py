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

    # these should be allowed due to TID253 top-level ban
    import foo_banned
    import foo_banned as renamed
    from pkg import bar_banned
    from pkg import bar_banned as renamed
    from pkg_banned import one as other, two, three

    # this should still trigger an error due to multiple imports
    from pkg import foo_allowed, bar_banned # [import-outside-toplevel]

class ClassWithImports:
    import tokenize  # [import-outside-toplevel]

    def __init__(self):
        import trace  # [import-outside-toplevel]

        # these should be allowed due to TID253 top-level ban
        import foo_banned
        import foo_banned as renamed
        from pkg import bar_banned
        from pkg import bar_banned as renamed
        from pkg_banned import one as other, two, three

        # this should still trigger an error due to multiple imports
        from pkg import foo_allowed, bar_banned # [import-outside-toplevel]
