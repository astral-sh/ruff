"""Compound statements with no body should be written on one line."""

while True:
    ...

if True:
    # comment
    ...

with True:
    ...  # comment

for i in []:
    ...

for i in []:
    # comment
    ...

for i in []:
    ... # comment

if True:
    ...

if True:
    # comment
    ...

if True:
    ... # comment

with True:
    ...

with True:
    # comment
    ...

with True:
    ... # comment

match x:
    case 1:
        ...
    case 2:
        # comment
        ...
    case 3:
        ... # comment
