### Errors

if 1 > 2:
    a: int = 0
else:
    a = 1


if 1 > 2:
    a: str = 0
else:
    a = 1


if 1 > 2:
    a: int = '0'
else:
    a = 1


if a:
    b: int = a
else:
    b = 42


if not a:
    b: int = a
else:
    b = 42


### No errors

if 1 > 2:
    a: int = 0
else:
    a: int = 1


if 1 > 2:
    a = 0
else:
    a: int = 1
