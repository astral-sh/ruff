# Tests for zero-argument super() on Python >= 3.12 (PEP 709)
class Base:
    def transform(self, x):
        return x


class TestSuper(Base):
    def test_append_zero_arg_super(self, items):
        result = []
        for x in items:
            result.append(super().transform(x))  # PERF401

    def test_extend_zero_arg_super(self, items):
        result = [1]
        for x in items:
            result.append(super().transform(x))  # OK