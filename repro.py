"""Minimal repro: reveal_type suppressed in unreachable code when imported.

When `reveal_type` is imported (from typing or typing_extensions), it resolves
to `Never` in unreachable code regions, so the KnownFunction::RevealType
special handling never triggers and no "Revealed type" diagnostic is emitted.

The same applies to assert_type and assert_never — all imported names resolve
to `Never` in unreachable code, so their special diagnostics are suppressed.

The builtin fallback (using bare `reveal_type` without importing) does NOT
have this problem — it always resolves correctly via the special-case in
infer_name_load.
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
        reveal_type(x)       # expected: revealed type `Never`
                              # actual:   nothing

        assert_type(x, int)   # expected: type-assertion-failure
                              # actual:   nothing

        assert_never(x)       # expected: no error (x is Never), but the
                              # call itself is completely invisible


# ---- Contrast: builtin reveal_type works in unreachable code ----
# To test this, comment out the `from typing import reveal_type` above
# and uncomment the function below. The builtin fallback DOES emit
# diagnostics in unreachable code.
#
# def builtin_works():
#     x: int = 1
#     return
#     reveal_type(x)   # OK: emits "Revealed type: Never" + undefined-reveal warning
