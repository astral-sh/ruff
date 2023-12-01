from typing import ParamSpec, TypeVar

# Errors.

T = TypeVar("T", covariant=True, contravariant=True)
T = TypeVar(name="T", covariant=True, contravariant=True)

T = ParamSpec("T", covariant=True, contravariant=True)
T = ParamSpec(name="T", covariant=True, contravariant=True)

# Non-errors.

T = TypeVar("T")
T = TypeVar("T", covariant=False)
T = TypeVar("T", contravariant=False)
T = TypeVar("T", covariant=False, contravariant=False)
T = TypeVar("T", covariant=True)
T = TypeVar("T", covariant=True, contravariant=False)
T = TypeVar(name="T", covariant=True, contravariant=False)
T = TypeVar(name="T", covariant=True)
T = TypeVar("T", contravariant=True)
T = TypeVar("T", covariant=False, contravariant=True)
T = TypeVar(name="T", covariant=False, contravariant=True)
T = TypeVar(name="T", contravariant=True)

T = ParamSpec("T")
T = ParamSpec("T", covariant=False)
T = ParamSpec("T", contravariant=False)
T = ParamSpec("T", covariant=False, contravariant=False)
T = ParamSpec("T", covariant=True)
T = ParamSpec("T", covariant=True, contravariant=False)
T = ParamSpec(name="T", covariant=True, contravariant=False)
T = ParamSpec(name="T", covariant=True)
T = ParamSpec("T", contravariant=True)
T = ParamSpec("T", covariant=False, contravariant=True)
T = ParamSpec(name="T", covariant=False, contravariant=True)
T = ParamSpec(name="T", contravariant=True)
