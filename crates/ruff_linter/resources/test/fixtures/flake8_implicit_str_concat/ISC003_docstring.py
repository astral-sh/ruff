# Regression tests for https://github.com/astral-sh/ruff/issues/27979.

# Module docstring position: fix is unsafe.
(
    "docstring"
    + "?"
)

# Not a docstring position: not the first statement in the module body.
x = 1
(
    "not"
    + " a docstring"
)


def function_docstring():
    # Function docstring position: fix is unsafe.
    (
        "docstring"
        + "?"
    )
    return __doc__


class ClassDocstring:
    # Class docstring position: fix is unsafe.
    (
        "docstring"
        + "?"
    )

    def method_docstring(self):
        # Method docstring position: fix is unsafe.
        (
            "docstring"
            + "?"
        )
        return self.__doc__


def f_string_in_docstring_position():
    # F-strings cannot be docstrings: fix is safe.
    (
        f"not"
        + " a docstring"
    )


def bytes_in_docstring_position():
    # Byte strings cannot be docstrings: fix is safe.
    (
        b"not"
        + b" a docstring"
    )


def nested_in_expression():
    # Not a docstring position: the concatenation is nested inside an
    # expression.
    print(
        "not"
        + " a docstring"
    )
