# Zero-argument super() is safe in comprehensions on Python >= 3.12 (PEP 709)
# (https://github.com/astral-sh/ruff/issues/27668)
class Base:
    def filter(self, x):
        return True


class TestDictSuper(Base):
    def test_if_zero_arg_super(self, fruit):
        result = {}
        for idx, name in enumerate(fruit):
            if super().filter(idx):
                result[idx] = name  # PERF403
