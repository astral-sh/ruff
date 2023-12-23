def ufoo() -> None:
    print(u"Hello, world!")  # PLW1406

def bfoo() -> None:
    print(b"Hello, world!")  # OK

def rfoo() -> None:
    print(r"Hello, world!")  # OK

def ffoo() -> None:
    print(f"Hello, world!")  # OK

def foo() -> None:
    print("Hello, world!")  # OK
    print("u")  # OK
    # ^ originally, there was a bug for strings that started with `u` due to not looking for the StringLiteral node
