import typing
from typing import cast

def foo():
    # For control var used outside block
    for event in []:
        pass

    _ = event

    # Tuple destructuring
    for a, b, c in []:
        pass

    _ = a
    _ = b
    _ = c


    # Array destructuring
    for [d, e, f] in []:
        pass

    _ = d
    _ = e
    _ = f

    # # With -> for, variable reused
    # with None as i:
    #     for i in []:  # error
    #         pass

    # # For -> with, variable reused
    # for i in []:
    #     with None as i:  # error
    #         pass
