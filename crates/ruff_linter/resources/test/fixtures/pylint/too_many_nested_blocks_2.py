def foo():
    while a:                              # \
        if b:                             # |
            for c in range(3):            # | These should not be reported,
                if d:                     # | as they don't exceed the max depth.
                    while e:              # |
                        if f:             # /

                            for g in z:   # This statement is the first to exceed the limit.
                                print(p)  # Thus, it is reported but not any of its substatements.
                                pass      #

                            with y:       # The former statement was already reported.
                                print(x)  # Thus, reporting these is redundant.
                            print(u)      #

        else:                             # Other blocks of an ancestor statement
            print(q)                      # are also not reported.


def foo():
    while a:                              # \
        if b:                             # |
            for c in range(3):            # | These should not be reported,
                if d:                     # | as they don't exceed the max depth.
                    while e:              # |
                        if f:             # /

                            if x == y:    # This statement is the first to exceed the limit.
                                print(p)  # It is therefore reported.

                            elif y > x:   # This block belongs to the same statement,
                                print(p)  # and so it is not reported on its own.
