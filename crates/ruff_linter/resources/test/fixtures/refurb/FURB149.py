failed = True

if True == failed:  # FURB149
    print("You failed")

if True != failed:  # FURB149
    print("You did not fail")

if False == failed:  # FURB149
    print("You did not fail")

if False != failed:  # FURB149
    print("You failed")

if True is failed:  # FURB149
    print("You failed")

if True is not failed:  # FURB149
    print("You did not fail")

if False is failed:  # FURB149
    print("You did not fail")

if False is not failed:  # FURB149
    print("You failed")

if failed is True:  # FURB149
    print("You failed")

if failed is not True:  # FURB149
    print("You did not fail")

if failed is False:  # FURB149
    print("You did not fail")

if failed is not False:  # FURB149
    print("You failed")

if failed == True:  # FURB149
    print("You failed")

if failed != True:  # FURB149
    print("You did not fail")

if failed == False:  # FURB149
    print("You did not fail")

if failed != False:  # FURB149
    print("You failed")

if (failed == True) or (failed == False):  # FURB149
    print("wat")


# OK
if failed:
    print("You failed")

if not failed:
    print("You did not fail")

if True is False:
    ...

if True is not False:
    ...

if True == False:
    ...

if True != False:
    ...
