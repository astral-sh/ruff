from typing import Any


d = {1: 1, 2: 2}
d_tuple = {(1, 2): 3, (4, 5): 6}
d_tuple_annotated: Any = {(1, 2): 3, (4, 5): 6}
d_tuple_incorrect_tuple = {(1,): 3, (4, 5): 6}
l = [1, 2]
s1 = {1, 2}
s2 = {1, 2, 3}

# Errors
for k, v in d:
    pass

for k, v in d_tuple_incorrect_tuple:
    pass


# Non errors
for k, v in d.items():
    pass
for k in d.keys():
    pass
for i, v in enumerate(l):
    pass
for i, v in s1.intersection(s2):
    pass
for a, b in d_tuple: 
    pass
for a, b in d_tuple_annotated: 
    pass
