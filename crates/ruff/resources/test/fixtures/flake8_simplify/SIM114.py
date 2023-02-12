# Errors
if a:
    b
elif c:
    b

if x == 1:
    for _ in range(20):
        print("hello")
elif x == 2:
    for _ in range(20):
        print("hello")

if x == 1:
    if True:
        for _ in range(20):
            print("hello")
elif x == 2:
    if True:
        for _ in range(20):
            print("hello")

if x == 1:
    if True:
        for _ in range(20):
            print("hello")
    elif False:
        for _ in range(20):
            print("hello")
elif x == 2:
    if True:
        for _ in range(20):
            print("hello")
    elif False:
        for _ in range(20):
            print("hello")

if (
    x == 1
    and y == 2
    and z == 3
    and a == 4
    and b == 5
    and c == 6
    and d == 7
    and e == 8
    and f == 9
    and g == 10
    and h == 11
    and i == 12
    and j == 13
    and k == 14
):
    pass
elif 1 == 2:
    pass

if result.eofs == "O":
    pass
elif result.eofs == "S":
    skipped = 1
elif result.eofs == "F":
    errors = 1
elif result.eofs == "E":
    errors = 1


# OK
def complicated_calc(*arg, **kwargs):
    return 42


def foo(p):
    if p == 2:
        return complicated_calc(microsecond=0)
    elif p == 3:
        return complicated_calc(microsecond=0, second=0)
    return None


a = False
b = True
c = True

if a:
    z = 1
elif b:
    z = 2
elif c:
    z = 1

# False negative (or arguably a different rule)
if result.eofs == "F":
    errors = 1
else:
    errors = 1
