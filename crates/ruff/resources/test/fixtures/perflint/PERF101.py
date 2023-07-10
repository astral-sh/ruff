foo_tuple = (1, 2, 3)
foo_list = [1, 2, 3]
foo_set = {1, 2, 3}
foo_dict = {1: 2, 3: 4}
foo_int = 123

for i in list(foo_tuple):  # PERF101
    pass

for i in list(foo_list):  # PERF101
    pass

for i in list(foo_set):  # PERF101
    pass

for i in list((1, 2, 3)):  # PERF101
    pass

for i in list([1, 2, 3]):  # PERF101
    pass

for i in list({1, 2, 3}):  # PERF101
    pass

for i in list(
    {
    1,
    2,
    3,
    }
):
    pass

for i in list( # Comment
    {1, 2, 3}
):  # PERF101
    pass

for i in list(foo_dict):  # Ok
    pass

for i in list(1):  # Ok
    pass

for i in list(foo_int):  # Ok
    pass


import itertools

for i in itertools.product(foo_int):  # Ok
    pass
