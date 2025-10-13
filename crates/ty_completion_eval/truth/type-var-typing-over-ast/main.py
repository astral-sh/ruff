# This one demands that `TypeVa` complete to `typing.TypeVar`
# even though there is also an `ast.TypeVar`. Getting this one
# right seems tricky, and probably requires module-specific
# heuristics.
#
# ref: https://github.com/astral-sh/ty/issues/1274#issuecomment-3345884227
TypeVa<CURSOR: typing.TypeVar>

# This is a similar case of `ctypes.cast` being preferred over
# `typing.cast`. Maybe `typing` should just get a slightly higher
# weight than most other stdlib modules?
cas<CURSOR: typing.cast>
