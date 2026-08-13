# https://github.com/astral-sh/ruff/issues/27668
# On Python < 3.12 a comprehension has its own scope and cannot see the enclosing
# method's `__class__` cell, so a bare `super()` (or a `__class__` reference) moved
# into a comprehension raises at runtime. RUF... PERF401 must not suggest the
# conversion in that case (it is suggested normally on 3.12+).
class A:
    def foo(self):
        return 1


class B(A):
    def with_super(self):
        result = []
        for _ in range(10):
            result.append(super().foo())  # zero-arg super(): unsafe below 3.12
        return result

    def with_class_cell(self):
        result = []
        for _ in range(10):
            result.append(__class__.__name__)  # __class__ reference: unsafe below 3.12
        return result

    def with_super_in_filter(self):
        result = []
        for x in range(10):
            if super().foo():  # zero-arg super() in the filter: unsafe below 3.12
                result.append(x)
        return result

    def with_explicit_super(self):
        result = []
        for _ in range(10):
            result.append(super(B, self).foo())  # explicit args: fine, still flagged
        return result

    def without_super(self):
        result = []
        for x in range(10):
            result.append(x + 1)  # no class cell: always flagged
        return result
