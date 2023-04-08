field1: str
field2: str | str # PYI016

def my_func(a: str | str) -> int | int:
    c = a | a
    print(a, c)
