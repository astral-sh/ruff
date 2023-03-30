# SIM102
if a:
    if b:
        c

# SIM102
if a:
    if b:
        if c:
            d

# SIM102
if a:
    pass
elif b:
    if c:
        d

# SIM102
if a:
    # Unfixable due to placement of this comment.
    if b:
        c

# SIM102
if a:
    if b:
        # Fixable due to placement of this comment.
        c

# OK
if a:
    if b:
        c
    else:
        d

# OK
if __name__ == "__main__":
    if foo():
        ...

# OK
if a:
    d
    if b:
        c

while x > 0:
    # SIM102
    if y > 0:
        if z > 0:
            """this
is valid"""

            """the indentation on
            this line is significant"""

            "this is" \
"allowed too"

            ("so is"
"this for some reason")


# SIM102
if x > 0:
    if y > 0:
        """this
is valid"""

        """the indentation on
        this line is significant"""

        "this is" \
"allowed too"

        ("so is"
"this for some reason")

while x > 0:
    # SIM102
    if node.module:
        if node.module == "multiprocessing" or node.module.startswith(
            "multiprocessing."
        ):
            print("Bad module!")

# SIM102
if node.module:
    if node.module == "multiprocessing" or node.module.startswith(
        "multiprocessing."
    ):
        print("Bad module!")


# OK
if a:
    if b:
        print("foo")
else:
    print("bar")

# OK
if a:
    if b:
        if c:
            print("foo")
        else:
            print("bar")
else:
    print("bar")

# OK
if a:
    # SIM 102
    if b:
        if c:
            print("foo")
else:
    print("bar")


# OK
if a:
    if b:
        if c:
            print("foo")
        print("baz")
else:
    print("bar")


# OK
if False:
    if a:
        pass


# OK
if True:
    if a:
        pass
