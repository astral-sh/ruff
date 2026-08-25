# https://github.com/astral-sh/ruff/issues/27979

# Unsafe: the fix would create a module docstring.
(
    "module "
    + "docstring"
)


# Unsafe: the fix would create a class docstring.
class ClassWithExplicitConcatenation:
    (
        "class "
        + "docstring"
    )


# Unsafe: the fix would create a function docstring.
def function_with_explicit_concatenation():
    (
        "function "
        + "docstring"
    )


# Safe: the concatenation isn't itself the first expression statement.
def function_with_safe_concatenation():
    print(
        "hello "
        + "world"
    )
