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

for i in list(foo_list):  # Ok
    foo_list.append(i + 1)

for i in list(foo_list):  # PERF101
    # Make sure we match the correct list
    other_list.append(i + 1)

for i in list(foo_tuple):  # Ok
    foo_tuple.append(i + 1)

for i in list(foo_set):  # Ok
    foo_set.append(i + 1)

x, y, nested_tuple = (1, 2, (3, 4, 5))

for i in list(nested_tuple):  # PERF101
    pass
