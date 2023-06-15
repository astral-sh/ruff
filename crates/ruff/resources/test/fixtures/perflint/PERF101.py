from typing import List, Dict, Any

foo_tuple = (1, 2, 3)
foo_list = [1, 2, 3]
foo_set = {1, 2, 3}
foo_dict = {1: 2, 3: 4}


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


def foo(items: List[int]):
    for item in list(items):  # PERF101
        pass


def bar(items: Dict[str, Any]):
    for item in list(items):  # Ok
        pass


for i in list(foo_dict):  # Ok
    pass

