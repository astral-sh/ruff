# Regression test for https://github.com/astral-sh/ruff/issues/23185.
#
# With `future-annotations = true` and `strict = false`, a typing-only import
# that is implicitly loaded by a valid runtime-used sibling import from the
# same module should NOT be flagged as TC001/TC002/TC003.


def from_collections_abc():
    # `Set` is used at runtime, so `Iterable` is implicitly available at runtime.
    from collections.abc import Iterable, Set

    def f(x: Iterable[int]) -> Set[int]:
        return Set(x)


def from_pkg():
    # `pkg` is used at runtime, `A` is typing-only but implicitly runtime-available
    # via `pkg`.
    import pkg
    from pkg import A

    def test(value: A):
        return pkg.B()


def submodule_implicit():
    # `pkg.bar` is used at runtime, which implicitly makes `pkg` available.
    import pkg
    import pkg.bar

    def test(value: pkg.A):
        return pkg.bar.B()
