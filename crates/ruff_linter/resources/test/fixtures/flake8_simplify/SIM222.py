if a or True:  # SIM222
    pass

if (a or b) or True:  # SIM222
    pass

if a or (b or True):  # SIM222
    pass

if a and True:  # OK
    pass

if True:  # OK
    pass


def validate(self, value):
    return json.loads(value) or True  # OK


if a or f() or b or g() or True:  # OK
    pass

if a or f() or True or g() or b:  # SIM222
    pass

if True or f() or a or g() or b:  # SIM222
    pass

if a or True or f() or b or g():  # SIM222
    pass


if a and f() and b and g() and False:  # OK
    pass

if a and f() and False and g() and b:  # OK
    pass

if False and f() and a and g() and b:  # OK
    pass

if a and False and f() and b and g():  # OK
    pass


a or "" or True  # SIM222

a or "foo" or True or "bar"  # SIM222

a or 0 or True  # SIM222

a or 1 or True or 2  # SIM222

a or 0.0 or True  # SIM222

a or 0.1 or True or 0.2  # SIM222

a or [] or True  # SIM222

a or list([]) or True  # SIM222

a or [1] or True or [2]  # SIM222

a or list([1]) or True or list([2])  # SIM222

a or {} or True  # SIM222

a or dict() or True  # SIM222

a or {1: 1} or True or {2: 2}  # SIM222

a or dict({1: 1}) or True or dict({2: 2})  # SIM222

a or set() or True  # SIM222

a or set(set()) or True  # SIM222

a or {1} or True or {2}  # SIM222

a or set({1}) or True or set({2})  # SIM222

a or () or True  # SIM222

a or tuple(()) or True  # SIM222

a or (1,) or True or (2,)  # SIM222

a or tuple((1,)) or True or tuple((2,))  # SIM222

a or frozenset() or True  # SIM222

a or frozenset(frozenset()) or True  # SIM222

a or frozenset({1}) or True or frozenset({2})  # SIM222

a or frozenset(frozenset({1})) or True or frozenset(frozenset({2}))  # SIM222


# Inside test `a` is simplified.

bool(a or [1] or True or [2])  # SIM222

assert a or [1] or True or [2]  # SIM222

if (a or [1] or True or [2]) and (a or [1] or True or [2]):  # SIM222
    pass

0 if a or [1] or True or [2] else 1  # SIM222

while a or [1] or True or [2]:  # SIM222
    pass

[
    0
    for a in range(10)
    for b in range(10)
    if a or [1] or True or [2]  # SIM222
    if b or [1] or True or [2]  # SIM222
]

{
    0
    for a in range(10)
    for b in range(10)
    if a or [1] or True or [2]  # SIM222
    if b or [1] or True or [2]  # SIM222
}

{
    0: 0
    for a in range(10)
    for b in range(10)
    if a or [1] or True or [2]  # SIM222
    if b or [1] or True or [2]  # SIM222
}

(
    0
    for a in range(10)
    for b in range(10)
    if a or [1] or True or [2]  # SIM222
    if b or [1] or True or [2]  # SIM222
)

# Outside test `a` is not simplified.

a or [1] or True or [2]  # SIM222

if (a or [1] or True or [2]) == (a or [1]):  # SIM222
    pass

if f(a or [1] or True or [2]):  # SIM222
    pass

# Regression test for: https://github.com/astral-sh/ruff/issues/7099
def secondToTime(s0: int) -> (int, int, int) or str:
    m, s = divmod(s0, 60)


def secondToTime(s0: int) -> ((int, int, int) or str):
    m, s = divmod(s0, 60)


# Regression test for: https://github.com/astral-sh/ruff/issues/9479
print(f"{a}{b}" or "bar")
print(f"{a}{''}" or "bar")
print(f"{''}{''}" or "bar")
print(f"{1}{''}" or "bar")
