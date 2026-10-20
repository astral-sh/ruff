# Regression test for https://github.com/astral-sh/ty/issues/4615

(decorator := (lambda: replacement))

@0
@decorator
class C:
    pass

(replacement := (lambda: C))
