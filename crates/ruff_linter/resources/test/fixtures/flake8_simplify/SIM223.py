if a and False:  # SIM223
    pass

if (a or b) and False:  # SIM223
    pass

if a or (b and False):  # SIM223
    pass

if a or False:
    pass

if False:
    pass

if a and f() and b and g() and False:  # OK
    pass

if a and f() and False and g() and b:  # SIM223
    pass

if False and f() and a and g() and b:  # SIM223
    pass

if a and False and f() and b and g():  # SIM223
    pass


if a or f() or b or g() or True:  # OK
    pass

if a or f() or True or g() or b:  # OK
    pass

if True or f() or a or g() or b:  # OK
    pass

if a or True or f() or b or g():  # OK
    pass


a and "" and False  # SIM223

a and "foo" and False and "bar"  # SIM223

a and 0 and False  # SIM223

a and 1 and False and 2  # SIM223

a and 0.0 and False  # SIM223

a and 0.1 and False and 0.2  # SIM223

a and [] and False  # SIM223

a and list([]) and False  # SIM223

a and [1] and False and [2]  # SIM223

a and list([1]) and False and list([2])  # SIM223

a and {} and False  # SIM223

a and dict() and False  # SIM223

a and {1: 1} and False and {2: 2}  # SIM223

a and dict({1: 1}) and False and dict({2: 2})  # SIM223

a and set() and False  # SIM223

a and set(set()) and False  # SIM223

a and {1} and False and {2}  # SIM223

a and set({1}) and False and set({2})  # SIM223

a and () and False  # SIM222

a and tuple(()) and False  # SIM222

a and (1,) and False and (2,)  # SIM222

a and tuple((1,)) and False and tuple((2,))  # SIM222

a and frozenset() and False  # SIM222

a and frozenset(frozenset()) and False  # SIM222

a and frozenset({1}) and False and frozenset({2})  # SIM222

a and frozenset(frozenset({1})) and False and frozenset(frozenset({2}))  # SIM222


# Inside test `a` is simplified.

bool(a and [] and False and [])  # SIM223

assert a and [] and False and []  # SIM223

if (a and [] and False and []) or (a and [] and False and []):  # SIM223
    pass

0 if a and [] and False and [] else 1  # SIM222

while a and [] and False and []:  # SIM223
    pass

[
    0
    for a in range(10)
    for b in range(10)
    if a and [] and False and []  # SIM223
    if b and [] and False and []  # SIM223
]

{
    0
    for a in range(10)
    for b in range(10)
    if a and [] and False and []  # SIM223
    if b and [] and False and []  # SIM223
}

{
    0: 0
    for a in range(10)
    for b in range(10)
    if a and [] and False and []  # SIM223
    if b and [] and False and []  # SIM223
}

(
    0
    for a in range(10)
    for b in range(10)
    if a and [] and False and []  # SIM223
    if b and [] and False and []  # SIM223
)

# Outside test `a` is not simplified.

a and [] and False and []  # SIM223

if (a and [] and False and []) == (a and []):  # SIM223
    pass

if f(a and [] and False and []):  # SIM223
    pass

# Regression test for: https://github.com/astral-sh/ruff/issues/9479
print(f"{a}{b}" and "bar")
print(f"{a}{''}" and "bar")
print(f"{''}{''}" and "bar")
print(f"{1}{''}" and "bar")
