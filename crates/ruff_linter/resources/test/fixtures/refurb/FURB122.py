from pathlib import Path

lines = ["line 1", "line 2", "line 3"]


# Errors

def _():
    with open("file", "w") as f:
        for line in lines:
            f.write(line)


def _():
    other_line = "other line"
    with Path("file").open("w") as f:
        for line in lines:
            f.write(other_line)


def _():
    with Path("file").open("w") as f:
        for line in lines:
            f.write(line)


def _():
    with Path("file").open("wb") as f:
        for line in lines:
            f.write(line.encode())


def _():
    with Path("file").open("w") as f:
        for line in lines:
            f.write(line.upper())


def _():
    with Path("file").open("w") as f:
        pass

        for line in lines:
            f.write(line)


def _():
    # Offer unsafe fix if it would delete comments
    with open("file","w") as f:
        for line in lines:
            # a really important comment
            f.write(line)


def _():
    with open("file", "w") as f:
        for () in a:
            f.write(())


def _():
    with open("file", "w") as f:
        for a, b, c in d:
            f.write((a, b))


def _():
    with open("file", "w") as f:
        for [(), [a.b], (c,)] in d:
            f.write(())


def _():
    with open("file", "w") as f:
        for [([([a[b]],)],), [], (c[d],)] in e:
            f.write(())


def _():
    # https://github.com/astral-sh/ruff/issues/15936
    with open("file", "w") as f:
        for char in "a", "b":
            f.write(char)

def _():
    # https://github.com/astral-sh/ruff/issues/15936
    with open("file", "w") as f:
        for char in "a", "b":
            f.write(f"{char}")

def _():
    with open("file", "w") as f:
        for char in (
            "a",  # Comment
            "b"
        ):
            f.write(f"{char}")


# OK

def _():
    for line in lines:
        Path("file").open("w").write(line)


def _():
    with Path("file").open("w") as f:
        for line in lines:
            pass

            f.write(line)


def _():
    with Path("file").open("w") as f:
        for line in lines:
            f.write(line)
        else:
            pass


async def func():
    with Path("file").open("w") as f:
        async for line in lines:  # type: ignore
            f.write(line)


def _():
    with Path("file").open("w") as f:
        for line in lines:
            f.write()  # type: ignore


def _():
    with open("file", "w") as f:
        global CURRENT_LINE
        for CURRENT_LINE in lines:
            f.write(CURRENT_LINE)


def _():
    foo = 1
    def __():
        with open("file", "w") as f:
            nonlocal foo
            for foo in lines:
                f.write(foo)


def _():
    with open("file", "w") as f:
        line = ''
        for line in lines:
            f.write(line)


def _():
    with open("file", "w") as f:
        for a, b, c in d:
            f.write((a, b))
        print(a)


def _():
    with open("file", "w") as f:
        for [*[*a]], b, [[c]] in d:
            f.write((a, b))
        print(c)


def _():
    with open("file", "w") as f:
        global global_foo
        for [a, b, (global_foo, c)] in d:
            f.write((a, b))
