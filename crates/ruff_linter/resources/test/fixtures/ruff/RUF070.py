a = 1.0
b = 2.0
x = 99

# ok
if a is None: ...
if a == 1: ...
if a != b: ...
if a > 1.0: ...

# errors
assert x == 0.0
print(3.14 != a)
if x == 0.3: ...
if x ==       0.42: ...


def foo(a, b):
    return a == b - 0.1

print(x == 3.0 == 3.0)
print(1.0 == x == 2)

assert x == float(1)
assert (a + b) == 1.0
assert -x == 1.0
assert -x == -1.0
assert x**2 == 4.0
assert x / 2 == 1.5
assert (y := x + 1.0) == 2.0

[i for i in range(10) if i == 1.0]
{i for i in range(10) if i != 2.0}

assert x / 2 == 1
assert (x / 2) == (y / 3)

# ok
assert Path(__file__).parent / ".txt" == 1
