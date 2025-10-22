"""Test module for D1001 and D1002."""


# D1002: Module-level variables

# Should trigger D1002 - undocumented public module variable (annotated)
public_var: int = 1

public_var_without_type = 2

public_var_without_value: int

# Should NOT trigger - has docstring
documented_var: str = "hello"
"""This variable is documented."""

# Should NOT trigger - private variable
_private_var: int = 2

# Should NOT trigger - has docstring
documented_regular = 200
"""This is also documented."""

# Should NOT trigger - not a simple name
obj.attr = 5

# Should NOT trigger - not in __all__, so considered private
not_exported: int = 999


class MyClass:
    """A test class."""

    # D1001: Class attributes

    # Should trigger D1001 - undocumented public class attribute (annotated)
    class_var: int = 10

    # Should NOT trigger - has docstring
    documented_class_var: str = "test"
    """This class variable is documented."""

    # Should NOT trigger - private attribute
    _private_class_var: int = 20

    # Should trigger D1001 - undocumented public class attribute (regular)
    another_class_var = 30

    # Should NOT trigger - documented
    documented_regular_class = 40
    """Another documented class variable."""

    def __init__(self):
        # Should NOT trigger - instance variables in methods are not checked
        self.instance_var: int = 50

class _PrivateClass:
    # public member in private class does not trigger
    public_member: str


__all__ = ["public_var", "public_var_without_type", "public_var_without_value", "documented_regular", "MyClass", "_private_var", "_PrivateClass"]