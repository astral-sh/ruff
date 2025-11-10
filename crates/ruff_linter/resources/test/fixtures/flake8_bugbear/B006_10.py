# Module-level mutable "constant"
MY_SET = {"ABC", "DEF"}  # plain set
MY_LIST = [1, 2, 3]  # plain list
MY_DICT = {"key": "value"}  # plain dict

# NOT triggering B006 (correct - UPPER_CASE constants are excluded per PEP 8)
def func_A(s: set[str] = MY_SET):
    return s

# Triggering B006 (correct)
def func_B(s: set[str] = {"ABC", "DEF"}):
    return s

# NOT triggering B006 (correct - UPPER_CASE constants are excluded per PEP 8)
def func_C(items: list[int] = MY_LIST):
    return items

# NOT triggering B006 (correct - UPPER_CASE constants are excluded per PEP 8)
def func_D(data: dict[str, str] = MY_DICT):
    return data

