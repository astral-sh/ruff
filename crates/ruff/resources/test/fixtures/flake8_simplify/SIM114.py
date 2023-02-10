# These SHOULD change
if a:
    b
elif c:
    b

# These SHOULD NOT change

"""
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
"""
