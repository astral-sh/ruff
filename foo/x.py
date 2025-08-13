def foo(x: list[int]) -> list[int]:
    return list(map(lambda i: i + 1, x))

x = [1, 2, 3]
y = foo(x)
print(y)
