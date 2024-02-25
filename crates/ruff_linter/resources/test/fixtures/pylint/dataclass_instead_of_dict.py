from typing import Final

class Foo:
    BAR = "bar"

KEY_3 = "key_3"


# Subdicts have at least 1 common key
MAPPING_1 = {  # [consider-using-namedtuple-or-dataclass]
    "entry_1": {"key_1": 0, "key_2": 1, "key_diff_1": 2},
    "entry_2": {"key_1": 0, "key_2": 1, "key_diff_2": 3},
}
MAPPING_2 = {  # [consider-using-namedtuple-or-dataclass]
    "entry_1": {KEY_3: None, Foo.BAR: None},
    "entry_2": {KEY_3: None, Foo.BAR: None},
}

# ints are not valid fieldnames for namedtuples
MAPPING_3 = {
    "entry_1": {0: None, 1: None},
    "entry_2": {0: None, 1: None},
}

# Subdicts have no common keys
MAPPING_4 = {
    "entry_1": {"key_3": 0, "key_4": 1, "key_diff_1": 2},
    "entry_2": {"key_1": 0, "key_2": 1, "key_diff_2": 3},
}

def func():
    # Not in module scope
    mapping_5 = {
        "entry_1": {"key_1": 0, "key_2": 1},
        "entry_2": {"key_1": 0, "key_2": 1},
    }

    mapping_6: Final = {  # [consider-using-namedtuple-or-dataclass]
        "entry_1": {"key_1": 0, "key_2": 1},
        "entry_2": {"key_1": 0, "key_2": 1},
    }


# lists must have the same length
MAPPING_7 = {  # [consider-using-namedtuple-or-dataclass]
    "entry_1": [1, "a", set()],
    "entry_2": [2, "b", set()],
}
MAPPING_8 = {
    "entry_1": [],
    "entry_2": [],
}
MAPPING_9 = {
    "entry_1": [1],
    "entry_2": [2, "b"],
}
MAPPING_10 = {  # [consider-using-namedtuple-or-dataclass]
    "entry_1": (1, "a"),
    "entry_2": (2, "b"),
}

# No entry can't contain only dicts
MAPPING_11 = {
    "entry_1": [
        {"key_1": None, "key_2": None},
    ],
    "entry_2": [None]
}

# No either dict, tuple, or list as dict values
MAPPING_12 = {
    "entry_1": 1,
    "entry_2": 2,
}
MAPPING_13 = {
    "entry_1": "",
    "entry_2": "",
}

# Empty dicts do not trigger
MAPPING_14 = {}
MAPPING_15: dict[str, str] = {}
