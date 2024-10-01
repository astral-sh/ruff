from typing import Dict, List

names = {"key": "value"}
nums = [1, 2, 3]
x = 42
y = "hello"

# these should match

# FURB131
del nums[:]


# FURB131
del names[:]


# FURB131
del x, nums[:]


# FURB131
del y, names[:], x


def yes_one(x: list[int]):
    # FURB131
    del x[:]


def yes_two(x: dict[int, str]):
    # FURB131
    del x[:]


def yes_three(x: List[int]):
    # FURB131
    del x[:]


def yes_four(x: Dict[int, str]):
    # FURB131
    del x[:]


def yes_five(x: Dict[int, str]):
    # FURB131
    del x[:]

    x = 1


from builtins import list as SneakyList


sneaky = SneakyList()
# FURB131
del sneaky[:]

# these should not

del names["key"]
del nums[0]

x = 1
del x


del nums[1:2]


del nums[:2]
del nums[1:]
del nums[::2]


def no_one(param):
    del param[:]
