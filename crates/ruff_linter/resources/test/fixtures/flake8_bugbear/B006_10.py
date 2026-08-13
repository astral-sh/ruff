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
