# Regression test for https://github.com/astral-sh/ty/issues/4613

while previous := Decorated:
    @lambda value=missing: (lambda other=value: other)(value)
    @lambda cls: {key: 0}
    class Decorated:
        pass

key = previous
