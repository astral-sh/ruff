# Regression test for https://github.com/astral-sh/ty/issues/4615

@0
class C[**P]:
    pass

while -():
    (value := C[value])
