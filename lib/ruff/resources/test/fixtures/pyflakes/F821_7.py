"""Test: Mypy extensions."""

from mypy_extensions import DefaultNamedArg

# OK
_ = DefaultNamedArg(bool | None, name="some_prop_name")
_ = DefaultNamedArg(type=bool | None, name="some_prop_name")
_ = DefaultNamedArg(bool | None, "some_prop_name")

# Not OK
_ = DefaultNamedArg("Undefined", name="some_prop_name")
_ = DefaultNamedArg(type="Undefined", name="some_prop_name")
_ = DefaultNamedArg("Undefined", "some_prop_name")
