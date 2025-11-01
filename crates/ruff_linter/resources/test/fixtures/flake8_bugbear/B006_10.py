# Module-level mutable "constant"
MY_SET = {"ABC", "DEF"}  # plain set
MY_LIST = [1, 2, 3]  # plain list
MY_DICT = {"key": "value"}  # plain dict

# NOT triggering B006 (should trigger)
def func_A(s: set[str] = MY_SET):
    return s

# Triggering B006 (correct)
def func_B(s: set[str] = {"ABC", "DEF"}):
    return s

# Should trigger B006
def func_C(items: list[int] = MY_LIST):
    return items

# Should trigger B006
def func_D(data: dict[str, str] = MY_DICT):
    return data

