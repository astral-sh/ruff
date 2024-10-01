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

for i in list(foo_dict):  # OK
    pass

for i in list(1):  # OK
    pass

for i in list(foo_int):  # OK
    pass


import itertools

for i in itertools.product(foo_int):  # OK
    pass

for i in list(foo_list):  # OK
    foo_list.append(i + 1)

for i in list(foo_list):  # PERF101
    # Make sure we match the correct list
    other_list.append(i + 1)

for i in list(foo_tuple):  # OK
    foo_tuple.append(i + 1)

for i in list(foo_set):  # OK
    foo_set.append(i + 1)

x, y, nested_tuple = (1, 2, (3, 4, 5))

for i in list(nested_tuple):  # PERF101
    pass

for i in list(foo_list):  # OK
    if True:
        foo_list.append(i + 1)

for i in list(foo_list):  # OK
    if True:
        foo_list[i] = i + 1

for i in list(foo_list):  # OK
    if True:
        del foo_list[i + 1]

import builtins

for i in builtins.list(nested_tuple):  # PERF101
    pass
