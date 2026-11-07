# Zero-argument super() is safe in comprehensions on Python >= 3.12 (PEP 709)
# (https://github.com/astral-sh/ruff/issues/27668)
class Base:
    def transform(self, x):
        return x

    def filter(self, x):
        return True


class TestSuper(Base):
    def test_append_zero_arg_super(self, items):
        result = []
        for x in items:
            result.append(super().transform(x))  # PERF401

    def test_if_zero_arg_super(self, items):
        result = []
        for x in items:
            if super().filter(x):
                result.append(x)  # PERF401
