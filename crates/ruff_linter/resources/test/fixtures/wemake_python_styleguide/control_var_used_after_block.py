import typing
from typing import cast

for global_var in []:
    _ = global_var
    pass

_ = global_var

def foo():
    # For control var used outside block
    for event in []:
        _ = event
        pass

    _ = event

    for x in range(10):
        pass

    # Usage in another block
    for y in range(10):
        # x is used outside of the loop it was defined in (meant to use y)
        if x == 5:
            pass

    # Tuple destructuring
    for a, b, c in []:
        _ = a
        _ = b
        _ = c
        pass

    _ = a
    _ = b
    _ = c


    # Array destructuring
    for [d, e, f] in []:
        _ = d
        _ = e
        _ = f
        pass

    _ = d
    _ = e
    _ = f

    # with statement
    with None as i:
        _ = i
        pass

    _ = i

    # Nested blocks
    with None as i:
        for n in []:
            _ = n
            pass

        _ = n

    _ = n


