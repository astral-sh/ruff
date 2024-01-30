# Errors
if a:
    b
elif c:
    b

if a:  # we preserve comments, too!
    b
elif c:  # but not on the second branch
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
elif result.eofs == "X":
    errors = 1
elif result.eofs == "C":
    errors = 1


# OK
def complicated_calc(*arg, **kwargs):
    return 42


def func(p):
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

if a:
    # Ignore branches with diverging comments because it means we're repeating
    # the bodies because we have different reasons for each branch
    x = 1
elif c:
    x = 1


def func():
    a = True
    b = False
    if a > b:  # end-of-line
        return 3
    elif a == b:
        return 3
    elif a < b:  # end-of-line
        return 4
    elif b is None:
        return 4


def func():
    """Ensure that the named expression is parenthesized when merged."""
    a = True
    b = False
    if a > b:  # end-of-line
        return 3
    elif a := 1:
        return 3


if a:  # we preserve comments, too!
    b
elif c:  # but not on the second branch
    b


if a: b  # here's a comment
elif c: b


if(x > 200): pass
elif(100 < x and x < 200 and 300 < y and y < 800):
	pass
