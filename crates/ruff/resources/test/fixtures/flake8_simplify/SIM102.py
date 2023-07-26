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

# SIM102 (auto-fixable)
if node.module012345678:
    if node.module == "multiprocß9💣2ℝ" or node.module.startswith(
        "multiprocessing."
    ):
        print("Bad module!")

# SIM102 (not auto-fixable)
if node.module0123456789:
    if node.module == "multiprocß9💣2ℝ" or node.module.startswith(
        "multiprocessing."
    ):
        print("Bad module!")

# SIM102
# Regression test for https://github.com/apache/airflow/blob/145b16caaa43f0c42bffd97344df916c602cddde/airflow/configuration.py#L1161
if a:
    if b:
        if c:
            print("if")
elif d:
    print("elif")

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


# SIM102
def f():
    if a:
        pass
    elif b:
        if c:
            d
