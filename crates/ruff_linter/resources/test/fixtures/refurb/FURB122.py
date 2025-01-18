from pathlib import Path

lines = ["line 1", "line 2", "line 3"]

# Errors

with open("file", "w") as f:
    for line in lines:
        f.write(line)

other_line = "other line"
with Path("file").open("w") as f:
    for line in lines:
        f.write(other_line)

with Path("file").open("w") as f:
    for line in lines:
        f.write(line)

with Path("file").open("wb") as f:
    for line in lines:
        f.write(line.encode())

with Path("file").open("w") as f:
    for line in lines:
        f.write(line.upper())

with Path("file").open("w") as f:
    pass

    for line in lines:
        f.write(line)

# Offer unsafe fix if it would delete comments
with open("file","w") as f:
    for line in lines:
        # a really important comment
        f.write(line)

# OK

for line in lines:
    Path("file").open("w").write(line)

with Path("file").open("w") as f:
    for line in lines:
        pass

        f.write(line)

with Path("file").open("w") as f:
    for line in lines:
        f.write(line)
    else:
        pass


async def func():
    with Path("file").open("w") as f:
        async for line in lines:  # type: ignore
            f.write(line)


with Path("file").open("w") as f:
    for line in lines:
        f.write()  # type: ignore
