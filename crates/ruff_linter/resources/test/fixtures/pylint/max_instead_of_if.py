# pylint: disable=missing-docstring, invalid-name, too-few-public-methods, redefined-outer-name

value = 10
value2 = 0
value3 = 3

# Positive
if value < 10:  # [max-instead-of-if]
    value = 10

if value <= 10:  # [max-instead-of-if]
    value = 10

if value < value2:  # [max-instead-of-if]
    value = value2


class A:
    def __init__(self):
        self.value = 13


A1 = A()
if A1.value < 10:  # [max-instead-of-if]
    A1.value = 10


class AA:
    def __init__(self, value):
        self.value = value

    def __gt__(self, b):
        return self.value > b

    def __ge__(self, b):
        return self.value >= b

    def __lt__(self, b):
        return self.value < b

    def __le__(self, b):
        return self.value <= b


A1 = AA(0)
A2 = AA(3)

if A2 < A1:  # [max-instead-of-if]
    A2 = A1

if A2 <= A1:  # [max-instead-of-if]
    A2 = A1

# Negative
if value < 10:
    value = 2

if value <= 3:
    value = 5

if value < 10:
    value = 2
    value2 = 3

if value < value2:
    value = value3

if value < 5:
    value = value3

if 2 < value <= 3:
    value = 1

if value < 10:
    value = 10
else:
    value = 3

if value <= 3:
    value = 5
elif value == 3:
    value = 2
