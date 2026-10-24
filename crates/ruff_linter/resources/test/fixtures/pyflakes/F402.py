import os
import os.path as path
from os import path as function_path

if condition:
    from os import path as conditional_path


for os in range(3):
    pass

for path in range(3):
    pass

if condition:
    for path in range(3):
        pass


def nested(xs, c):
    if c:
        for function_path in xs:
            pass


for conditional_path in range(3):
    pass
