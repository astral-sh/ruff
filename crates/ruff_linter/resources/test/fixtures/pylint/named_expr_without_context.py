# Errors
(a := 42)
if True:
    (b := 1)


class Foo:
    (c := 1)


# OK
if a := 42:
    print("Success")

a = 0
while (a := a + 1) < 10:
    print("Correct")

a = (b := 1)
