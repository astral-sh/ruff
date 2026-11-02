# Regression test for https://github.com/astral-sh/ty/issues/4834

class C("", (lambda: value, (predicate := lambda: value))):
    pass

# Read the lambda before the loop adds the class as another possible value of predicate.
predicate
while True:
    assert predicate
    value = 0
    predicate = C
