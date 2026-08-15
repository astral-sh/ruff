# Regression test for https://github.com/astral-sh/ruff/issues/27022
# Moving a multiline-string default into the function body must not re-indent the
# interior lines of the string literal (which would change its value).


def f(value=["""first
second"""]):
    return value


class C:
    def m(self, value=["""first
second"""]):
        return value


# A multi-line default with no multi-line string is safe to re-indent, and should
# be aligned with the function body rather than left at its original indentation.
def g(
    value=[
        "first",
        "second",
    ],
):
    return value
