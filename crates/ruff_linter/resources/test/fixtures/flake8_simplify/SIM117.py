# SIM117
with A() as a:
    with B() as b:
        print("hello")

# SIM117
with A():
    with B():
        with C():
            print("hello")

# SIM117
with A() as a:
    # Unfixable due to placement of this comment.
    with B() as b:
        print("hello")

# SIM117
with A() as a:
    with B() as b:
        # Fixable due to placement of this comment.
        print("hello")

# OK
with A() as a:
    a()
    with B() as b:
        print("hello")

# OK
with A() as a:
    with B() as b:
        print("hello")
    a()

# OK, can't merge async with and with.
async with A() as a:
    with B() as b:
        print("hello")

# OK, can't merge async with and with.
with A() as a:
    async with B() as b:
        print("hello")

# SIM117
async with A() as a:
    async with B() as b:
        print("hello")

# SIM117 — nested async with under a sync with parent (issue #27800).
with A() as a:
    async with B() as b:
        async with C() as c:
            print(a, b, c)

# SIM117 — nested sync with under an async with parent.
async with A() as a:
    with B() as b:
        with C() as c:
            print(a, b, c)

while True:
    # SIM117
    with A() as a:
        with B() as b:
            """this
is valid"""

            """the indentation on
            this line is significant"""

            "this is" \
"allowed too"

            ("so is"
"this for some reason")

# SIM117
with (
    A() as a,
    B() as b,
):
    with C() as c:
        print("hello")

# SIM117
with A() as a:
    with (
        B() as b,
        C() as c,
    ):
        print("hello")

# SIM117
with (
    A() as a,
    B() as b,
):
    with (
        C() as c,
        D() as d,
    ):
        print("hello")

# SIM117 (auto-fixable)
with A("01ß9💣2ℝ8901ß9💣2ℝ8901ß9💣2ℝ89") as a:
    with B("01ß9💣2ℝ8901ß9💣2ℝ8901ß9💣2ℝ89") as b:
        print("hello")

# SIM117 (not auto-fixable too long)
with A("01ß9💣2ℝ8901ß9💣2ℝ8901ß9💣2ℝ890") as a:
    with B("01ß9💣2ℝ8901ß9💣2ℝ8901ß9💣2ℝ89") as b:
        print("hello")

# From issue #3025.
async def main():
    async with A() as a: # SIM117.
        async with B() as b:
            print("async-inside!")

    return 0

# OK. Can't merge across different kinds of with statements.
with a as a2:
    async with b as b2:
        with c as c2:
            async with d as d2:
                f(a2, b2, c2, d2)

# OK. Can't merge across different kinds of with statements.
async with b as b2:
    with c as c2:
        async with d as d2:
            f(b2, c2, d2)

# SIM117
with A() as a:
    with B() as b:
        type ListOrSet[T] = list[T] | set[T]

        class ClassA[T: str]:
            def method1(self) -> T:
                ...

        f" something { my_dict["key"] } something else "

        f"foo {f"bar {x}"} baz"

# Allow cascading for some statements.
import anyio
import asyncio
import trio

async with asyncio.timeout(1):
    async with A() as a:
        pass

async with A():
    async with asyncio.timeout(1):
        pass

async with asyncio.timeout(1):
    async with asyncio.timeout_at(1):
        async with anyio.CancelScope():
            async with anyio.fail_after(1):
                async with anyio.move_on_after(1):
                    async with trio.fail_after(1):
                        async with trio.fail_at(1):
                            async with trio.move_on_after(1):
                                async with trio.move_on_at(1):
                                    pass

# Do not suppress combination, if a context manager is already combined with another.
async with asyncio.timeout(1), A():
    async with B():
        pass

# SIM117 — nested async with under an exempt async parent (issue #27800 class).
async with asyncio.timeout(1):
    async with A() as a:
        async with B() as b:
            print(a, b)
