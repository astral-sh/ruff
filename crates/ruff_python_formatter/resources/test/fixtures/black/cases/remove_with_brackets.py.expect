with open("bla.txt"):
    pass

with open("bla.txt"), open("bla.txt"):
    pass

with open("bla.txt") as f:
    pass

# Remove brackets within alias expression
with open("bla.txt") as f:
    pass

# Remove brackets around one-line context managers
with open("bla.txt") as f, open("x"):
    pass

with open("bla.txt") as f, open("x"):
    pass

with CtxManager1() as example1, CtxManager2() as example2:
    ...

# Brackets remain when using magic comma
with (
    CtxManager1() as example1,
    CtxManager2() as example2,
):
    ...

# Brackets remain for multi-line context managers
with (
    CtxManager1() as example1,
    CtxManager2() as example2,
    CtxManager2() as example2,
    CtxManager2() as example2,
    CtxManager2() as example2,
):
    ...

# Don't touch assignment expressions
with (y := open("./test.py")) as f:
    pass

# Deeply nested examples
# N.B. Multiple brackets are only possible
# around the context manager itself.
# Only one brackets is allowed around the
# alias expression or comma-delimited context managers.
with open("bla.txt"):
    pass

with open("bla.txt"), open("bla.txt"):
    pass

with open("bla.txt") as f:
    pass

with open("bla.txt") as f:
    pass

with CtxManager1() as example1, CtxManager2() as example2:
    ...
