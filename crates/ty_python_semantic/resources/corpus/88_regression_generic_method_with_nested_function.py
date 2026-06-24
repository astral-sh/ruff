# Regression test for an issue that came up while working
# on https://github.com/astral-sh/ruff/pull/17769

class C:
    def method[T](self, x: T) -> T:
        def inner():
            self.attr = 1

C().attr
