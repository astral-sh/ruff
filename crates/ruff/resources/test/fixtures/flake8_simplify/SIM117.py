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
