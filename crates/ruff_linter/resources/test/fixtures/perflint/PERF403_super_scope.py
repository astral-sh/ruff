# https://github.com/astral-sh/ruff/issues/27668
# A dict comprehension has its own scope on Python < 3.12. PERF403 only copies loop
# variables into the key/value, but its `if` filter is an arbitrary expression, so a
# filter that relies on the `__class__` cell (a bare `super()` or `__class__`) cannot be
# moved into the comprehension below 3.12. PERF403 must not suggest the conversion then
# (it does on 3.12+).
class A:
    def cond(self):
        return True


class B(A):
    def filter_super(self):
        result = {}
        for i in range(10):
            if super().cond():  # zero-arg super() in the filter: unsafe below 3.12
                result[i] = i
        return result

    def filter_class_cell(self):
        result = {}
        for i in range(10):
            if __class__.__name__:  # __class__ reference in the filter: unsafe below 3.12
                result[i] = i
        return result

    def filter_explicit_super(self):
        result = {}
        for i in range(10):
            if super(B, self).cond():  # explicit args: fine, still flagged
                result[i] = i
        return result

    def filter_variadic_super(self):
        result = {}
        for i in range(10):
            # `super(*())` may expand to zero runtime arguments: unsafe below 3.12.
            if super(*()).cond():
                result[i] = i
        return result

    def filter_variadic_super_kwargs(self):
        result = {}
        for i in range(10):
            if super(**{}).cond():
                result[i] = i
        return result

    def filter_plain(self):
        result = {}
        for i in range(10):
            if i % 2:  # no class cell: always flagged
                result[i] = i
        return result
