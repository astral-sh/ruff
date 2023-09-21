def f1(x: str = "50 character stringggggggggggggggggggggggggggggggg") -> None: ...  # OK
def f2(
    x: str = "51 character stringgggggggggggggggggggggggggggggggg",  # Error: PYI053
) -> None: ...
def f3(
    x: str = "50 character stringgggggggggggggggggggggggggggggg\U0001f600",  # OK
) -> None: ...
def f4(
    x: str = "51 character stringggggggggggggggggggggggggggggggg\U0001f600",  # Error: PYI053
) -> None: ...
def f5(
    x: bytes = b"50 character byte stringgggggggggggggggggggggggggg",  # OK
) -> None: ...
def f6(
    x: bytes = b"51 character byte stringgggggggggggggggggggggggggg",  # Error: PYI053
) -> None: ...
def f7(
    x: bytes = b"50 character byte stringggggggggggggggggggggggggg\xff",  # OK
) -> None: ...
def f8(
    x: bytes = b"51 character byte stringgggggggggggggggggggggggggg\xff",  # Error: PYI053
) -> None: ...

foo: str = "50 character stringggggggggggggggggggggggggggggggg"  # OK

bar: str = "51 character stringgggggggggggggggggggggggggggggggg"  # Error: PYI053

baz: bytes = b"50 character byte stringgggggggggggggggggggggggggg"  # OK

qux: bytes = b"51 character byte stringggggggggggggggggggggggggggg\xff"  # Error: PYI053

class Demo:
    """Docstrings are excluded from this rule. Some padding."""  # OK

def func() -> None:
    """Docstrings are excluded from this rule. Some padding."""  # OK
