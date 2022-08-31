try:
    1 / 0
except ValueError as e:
    pass


try:
    1 / 0
except ValueError as e:
    print(e)


def f():
    x = 1
    y = 2
    z = x + y
