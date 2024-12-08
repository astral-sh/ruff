lambda print, copyright: print
lambda x, float, y: x + y
lambda min, max: min
lambda id: id
lambda dir: dir

# Ok for A006 - should trigger A002 instead
# https://github.com/astral-sh/ruff/issues/14135
def func1(str, /, type, *complex, Exception, **getattr):
    pass