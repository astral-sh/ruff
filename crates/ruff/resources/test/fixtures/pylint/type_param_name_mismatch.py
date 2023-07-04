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

T = ParamSpec("T")
T = ParamSpec(name="T")

T = NewType("T", int)
T = NewType(name="T", tp=int)

Ts = TypeVarTuple("Ts")
Ts = TypeVarTuple(name="Ts")

# Errors, but not covered by this rule.

# Non-string literal name.
T = TypeVar(some_str)
T = TypeVar(name=some_str)
T = TypeVar(1)
T = TypeVar(name=1)
T = ParamSpec(some_str)
T = ParamSpec(name=some_str)
T = ParamSpec(1)
T = ParamSpec(name=1)
T = NewType(some_str, int)
T = NewType(name=some_str, tp=int)
T = NewType(1, int)
T = NewType(name=1, tp=int)
Ts = TypeVarTuple(some_str)
Ts = TypeVarTuple(name=some_str)
Ts = TypeVarTuple(1)
Ts = TypeVarTuple(name=1)

# No names provided.
T = TypeVar()
T = ParamSpec()
T = NewType()
T = NewType(tp=int)
Ts = TypeVarTuple()
