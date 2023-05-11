def f24(
    x=42,  # Error PYI052
) -> None: ...

def f25(
    x=None,  # Error PYI052
) -> None: ...

def f26(
    x=False,  # Error PYI052
) -> None: ...

def f27(
    x=3.14,  # Error PYI052
) -> None: ...

def f28(
    x="hello",  # Error PYI052
) -> None: ...

def f29(
    x=[1, 2, "three"],  # Error PYI052
) -> None: ...

def f30(
    x=(1, 2, "three"),  # Error PYI052
) -> None: ...

def f31(
    x={1, 2, "three"},  # Error PYI052
) -> None: ...

def f32(
    x={"a": 1, "b": 2, "c": "three"},  # Error PYI052
) -> None: ...