"""Minimal repro: reveal_type / assert_type / assert_never suppressed in unreachable code.

ty emits NO diagnostic for these calls when it considers the enclosing
region unreachable, even though the user would expect to see output.
"""
from typing import reveal_type, assert_type, assert_never

# ---- Case 1: after a return statement ----
# A user adds a temporary `return` while debugging and wants to
# inspect a type below it.  They get complete silence.

def after_return():
    x: int = 1
    return
    reveal_type(x)      # expected: info diagnostic showing type
                         # actual:   nothing

# ---- Case 2: in a branch whose condition is always false ----
def always_false_branch():
    x: int = 1
    if False:
        reveal_type(x)   # expected: info diagnostic
                          # actual:   nothing

# ---- Case 3: else branch after exhaustive isinstance narrowing ----
# This is probably the most surprising variant: the user handles every
# case in an if/elif chain, and the else branch is unreachable.

def exhaustive_narrowing(x: int | str):
    if isinstance(x, int):
        return 1
    elif isinstance(x, str):
        return 2
    else:
        # The user put reveal_type here to see what's left.
        reveal_type(x)       # expected: revealed type `Never` (or some diagnostic)
                              # actual:   nothing

        # Similarly, assert_type and assert_never are silent:
        assert_type(x, int)   # expected: type-assertion-failure
                              # actual:   nothing
        assert_never(x)       # expected: no error (x is Never), but the
                              # call itself is completely invisible
