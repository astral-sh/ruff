# pylint: disable=missing-docstring, invalid-name, too-few-public-methods, redefined-outer-name


# the rule take care of the following cases:
#
# | Case  | Expression       | Fix           |
# |-------|------------------|---------------|
# | 1     | if a >= b: a = b | a = min(b, a) |
# | 2     | if a <= b: a = b | a = max(b, a) |
# | 3     | if a <= b: b = a | b = min(a, b) |
# | 4     | if a >= b: b = a | b = max(a, b) |
# | 5     | if a > b: a = b  | a = min(a, b) |
# | 6     | if a < b: a = b  | a = max(a, b) |
# | 7     | if a < b: b = a  | b = min(b, a) |
# | 8     | if a > b: b = a  | b = max(b, a) |

# the 8 base cases
a, b = [], []

# case 1: a = min(b, a)
if a >= b:
    a = b

# case 2: a = max(b, a)
if a <= b:
    a = b

# case 3: b = min(a, b)
if a <= b:
    b = a

# case 4: b = max(a, b)
if a >= b:
    b = a

# case 5: a = min(a, b)
if a > b:
    a = b

# case 6: a = max(a, b)
if a < b:
    a = b

# case 7: b = min(b, a)
if a < b:
    b = a

# case 8: b = max(b, a)
if a > b:
    b = a


# test cases with assigned variables and primitives
value = 10
value2 = 0
value3 = 3

# base case 6: value = max(value, 10)
if value < 10:
    value = 10

# base case 2: value = max(10, value)
if value <= 10:
    value = 10

# base case 6: value = max(value, value2)
if value < value2:
    value = value2

# base case 5: value = min(value, 10)
if value > 10:
    value = 10

# base case 1: value = min(10, value)
if value >= 10:
    value = 10

# base case 5: value = min(value, value2)
if value > value2:
    value = value2


# cases with calls
class A:
    def __init__(self):
        self.value = 13


A1 = A()


if A1.value < 10:
    A1.value = 10

if A1.value > 10:
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

if A2 > A1:  # [min-instead-of-if]
    A2 = A1

if A2 >= A1:  # [min-instead-of-if]
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

if value > 10:
    value = 2

if value >= 3:
    value = 5

if value > 10:
    value = 2
    value2 = 3

if value > value2:
    value = value3

if value > 5:
    value = value3

if 2 > value >= 3:
    value = 1

if value > 10:
    value = 10
else:
    value = 3

if value >= 3:
    value = 5
elif value == 3:
    value = 2

# Parenthesized expressions
if value.attr > 3:
    (
        value.
        attr
    ) = 3

class Foo:
    _min = 0
    _max = 0

    def foo(self, value) -> None:
        if value < self._min:
            self._min = value
        if value > self._max:
            self._max = value

        if self._min < value:
            self._min = value
        if self._max > value:
            self._max = value

        if value <= self._min:
            self._min = value
        if value >= self._max:
            self._max = value

        if self._min <= value:
            self._min = value
        if self._max >= value:
            self._max = value


counter = {"a": 0, "b": 0}

# base case 2: counter["a"] = max(counter["b"], counter["a"])
if counter["a"] <= counter["b"]:
    counter["a"] = counter["b"]

# case 3: counter["b"] = min(counter["a"], counter["b"])
if counter["a"] <= counter["b"]:
    counter["b"] = counter["a"]

# case 5: counter["a"] = min(counter["a"], counter["b"])
if counter["a"] > counter["b"]:
    counter["b"] = counter["a"]

# case 8: counter["a"] = max(counter["b"], counter["a"])
if counter["a"] > counter["b"]:
    counter["b"] = counter["a"]
