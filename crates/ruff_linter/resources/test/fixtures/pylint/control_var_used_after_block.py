import typing
from typing import cast

# TODO: Figure this out
# for global_var in []:
#     _ = global_var
#     pass

# _ = global_var

def foo():
    # For control var used outside block
    for event in []:
        _ = event
        pass

    _ = event

    # # Tuple destructuring
    for a, b, c in []:
        pass

    _ = a
    _ = b
    _ = c


    # # Array destructuring
    for [d, e, f] in []:
        pass

    _ = d
    _ = e
    _ = f

    with None as i:
        pass

    _ = i

    with None as i:
        for n in []:
            pass

        _ = n

    _ = n


