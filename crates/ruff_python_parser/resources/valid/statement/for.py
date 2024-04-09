for target in iter:
    pass

for target in (1, 2, 3):
    pass

for target.attr in call():
    pass

for target[0] in x.attr:
    pass

for target in x <= y:
    pass

for target in a and b:
    pass

for a, b, c, in iter:
    pass

for (a, b) in iter:
    pass

for target in *x.attr:
    pass

for target in [1, 2]:
    pass

for *target in a, b, c,:
    pass
else:
    pass

for target in *x | y: ...
for target in *await x: ...
for target in await x: ...
for target in lambda x: x: ...
for target in x if True else y: ...

if x:
    for target in iter:
        pass
# This `else` is not part of the `try` statement, so don't raise an error
else:
    pass
