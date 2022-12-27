from io import open

with open("f.txt") as f:
    print(f.read())

import io

with io.open("f.txt", mode="r", buffering=-1, **kwargs) as f:
    print(f.read())
