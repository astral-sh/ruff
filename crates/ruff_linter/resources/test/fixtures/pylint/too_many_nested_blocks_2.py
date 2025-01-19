def foo():
    while a:                              # \
        if b:                             # |
            for c in range(3):            # | These should not be reported,
                if d:                     # | as they don't exceed the max depth.
                    while e:              # |
                        if f:             # /

                            for g in z:   # This nested statement is the first to have
                                print(p)  # a substatement that exceed the limit.
                                pass      # It is reported but not any of its substatements.

                            with y:       # The former statement was already reported.
                                print(x)  # Thus, reporting these is redundant.
                            print(u)      #

        else:                             # \
            print(q)                      # |
            if h:                         # | Other blocks of an ancestor statement
                if i:                     # | are also not reported.
                    if j:                 # |
                        if k:             # /

                            if l:         # Unless they themselves have a branch
                                print()   # that exceeds the limit.


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


def foo():
    while a:                                      # \
        if b:                                     # |
            for c in range(3):                    # | These should not be reported,
                if d:                             # | as they don't exceed the max depth.
                    while e:                      # |
                        print()                   # |
                        if f:                     # /

                            if g:                 # This statement is the first nested statement to exceed the limit.
                                print()           # It is reported regardless of what comes before it.

                            else:                 # Secondary blocks of the same statement aren't.
                                print()           #

                                if i:             # But this branch is.
                                    print()       #
