from typing import TypeVar, ParamSpec, NewType, TypeVarTuple

# Errors.

X = TypeVar("T")
X = TypeVar(name="T")

Y = ParamSpec("T")
Y = ParamSpec(name="T")

Z = NewType("T", int)
Z = NewType(name="T", tp=int)

Ws = TypeVarTuple("Ts")
Ws = TypeVarTuple(name="Ts")

# Non-errors.

T = TypeVar("T")
T = TypeVar(name="T")
T = TypeVar(some_str)
T = TypeVar(name=some_str)

T = ParamSpec("T")
T = ParamSpec(name="T")
T = ParamSpec(some_str)
T = ParamSpec(name=some_str)

T = NewType("T", int)
T = NewType(name="T", tp=int)
T = NewType(some_str, int)
T = NewType(name=some_str, tp=int)

Ts = TypeVarTuple("Ts")
Ts = TypeVarTuple(name="Ts")
Ts = TypeVarTuple(some_str)
Ts = TypeVarTuple(name=some_str)

# Errors, but not covered by this rule.

T = TypeVar()  # No name provided.
T = ParamSpec()  # No name provided.
T = NewType()  # No name provided.
T = NewType(tp=int)  # No name provided (but type provided).
Ts = TypeVarTuple()  # No name provided.
