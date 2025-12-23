# Regression test for https://github.com/astral-sh/ruff/issues/17371
# panicked in commit d1088545a08aeb57b67ec1e3a7f5141159efefa5
# error message:
# dependency graph cycle when querying ClassType < 'db >::into_callable_(Id(1c00))

try:
    class foo[T: bar](object):
        pass
    bar = foo
except Exception:
    bar = lambda: 0
def bar():
    pass

@bar()
class bar:
    pass
