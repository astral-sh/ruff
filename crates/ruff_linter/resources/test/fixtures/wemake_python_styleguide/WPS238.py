from random import random


def f():
    if random() > 0.2:
        raise ValueError()

    if random() > 0.5:
        raise TypeError()


def g():
    if random() > 0.2:
        raise ValueError("hello")
    else:
        raise ValueError("world")

    if random() > 0.5:
        raise TypeError
    else:
        raise ValueError("violation")
