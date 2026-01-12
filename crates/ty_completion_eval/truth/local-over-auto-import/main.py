def foo(x):
    # We specifically want the local `x` to be
    # suggested first here, and NOT an `x` or an
    # `X` from some other module (via auto-import).
    # We'd also like this to come before `except`,
    # which is a keyword that contains `x`, but is
    # not an exact match (where as `x` is). `except`
    # also isn't legal in this context, although
    # that sort of context sensitivity is a bit
    # trickier.
    return x<CURSOR: x>
