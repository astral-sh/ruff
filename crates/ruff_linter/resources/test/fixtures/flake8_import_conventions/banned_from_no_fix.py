from __future__ import annotations


def target_conflict(math):
    from math import sqrt
    return sqrt(9)


def nested_target_conflict():
    from math import sqrt

    def inner(math):
        return sqrt(9)

    return inner


def reassigned_member():
    from math import sqrt
    result = sqrt(9)
    sqrt = lambda x: x
    return result, sqrt(9)


def deleted_member():
    from math import sqrt
    if condition:
        del sqrt


def quoted_annotation():
    from typing import Optional
    value: "Optional[int]" = None
    return value


def explicit_export():
    from math import sqrt as sqrt
    return sqrt(9)


def internal_comment():
    from math import (
        sqrt,  # Keep this explanation.
        ceil,
    )
    return sqrt(9), ceil(1.1)


def existing_import_comment():
    import math
    from math import sqrt  # Keep this comment.
    return sqrt(9)


def conditional_module_import():
    if condition:
        import math
    from math import sqrt
    return sqrt(9)


def later_module_import():
    from math import sqrt
    print(sqrt(9))
    import math


def rebound_module():
    import math
    from math import sqrt
    math = None
    return sqrt(9)


def global_declaration():
    global sqrt
    from math import sqrt
    return sqrt(9)


def nonlocal_declaration():
    from math import sqrt

    def inner():
        nonlocal sqrt
        return sqrt(9)

    return inner


def relative():
    from .math import sqrt
    return sqrt(9)


from math import sqrt as exported_sqrt
__all__ = ["exported_sqrt"]


from math import sqrt as global_root


def global_reference():
    global global_root
    return global_root(9)


def annotated_member():
    from math import sqrt
    sqrt: object
    return sqrt(9)


def rebound_existing_module():
    import math
    from math import sqrt

    def inner():
        nonlocal math
        math = None

    inner()
    return sqrt(9)