from typing import List


def func():
    pass


nums = []
nums2 = []
nums3: list[int] = func()
nums4: List[int]


class C:
    def append(self, x):
        pass


# Errors.


# FURB113
nums.append(1)
nums.append(2)
pass


# FURB113
nums3.append(1)
nums3.append(2)
pass


# FURB113
nums4.append(1)
nums4.append(2)
pass


# FURB113
nums.append(1)
nums2.append(1)
nums.append(2)
nums.append(3)
pass


# FURB113
nums.append(1)
nums2.append(1)
nums.append(2)
# FURB113
nums3.append(1)
nums.append(3)
# FURB113
nums4.append(1)
nums4.append(2)
nums3.append(2)
pass

# FURB113
nums.append(1)
nums.append(2)
nums.append(3)


if True:
    # FURB113
    nums.append(1)
    nums.append(2)


if True:
    # FURB113
    nums.append(1)
    nums.append(2)
    pass


if True:
    # FURB113
    nums.append(1)
    nums2.append(1)
    nums.append(2)
    nums.append(3)


def yes_one(x: list[int]):
    # FURB113
    x.append(1)
    x.append(2)


def yes_two(x: List[int]):
    # FURB113
    x.append(1)
    x.append(2)


def yes_three(*, x: list[int]):
    # FURB113
    x.append(1)
    x.append(2)


def yes_four(x: list[int], /):
    # FURB113
    x.append(1)
    x.append(2)


def yes_five(x: list[int], y: list[int]):
    # FURB113
    x.append(1)
    x.append(2)
    y.append(1)
    x.append(3)


def yes_six(x: list):
    # FURB113
    x.append(1)
    x.append(2)


if True:
    # FURB113
    nums.append(1)
    # comment
    nums.append(2)
    # comment
    nums.append(3)


# Non-errors.

nums.append(1)
pass
nums.append(2)


if True:
    nums.append(1)
    pass
    nums.append(2)


nums.append(1)
pass


nums.append(1)
nums2.append(2)


nums.copy()
nums.copy()


c = C()
c.append(1)
c.append(2)


def not_one(x):
    x.append(1)
    x.append(2)


def not_two(x: C):
    x.append(1)
    x.append(2)


# redefining a list variable with a new type shouldn't confuse ruff.
nums2 = C()
nums2.append(1)
nums2.append(2)
