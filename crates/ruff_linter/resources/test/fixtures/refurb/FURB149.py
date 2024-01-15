failed = True

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
