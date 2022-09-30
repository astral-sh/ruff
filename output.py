from collections import Counter
for x in range(5):
    print(x)
else:
    print('Nope!')
while False:
    print('False')
else:
    print('True')
    for x in range(5):
        print(x)
    else:
        print('Nope!')


def f(x: int, y: int, *, z: int) -> int:
    return (x + y + z)


with x as 1, y as 2:
    pass