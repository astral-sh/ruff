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

# OK
async with A() as a:
    with B() as b:
        print("hello")

# OK
with A() as a:
    async with B() as b:
        print("hello")

# OK
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
