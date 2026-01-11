# Regression test for https://github.com/astral-sh/ty/issues/1848

T = tuple[int, 'U']

class C(set['U']):
    pass

type U = T | C
