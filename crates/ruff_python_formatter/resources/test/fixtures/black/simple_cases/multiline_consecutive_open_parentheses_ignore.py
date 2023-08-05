# This is a regression test. Issue #3737

a = (  # type: ignore
    int(  # type: ignore
        int(  # type: ignore
            int(  # type: ignore
                6
            )
        )
    )
)

b = (
    int(
        6
    )
)

print(   "111") # type: ignore
print(   "111"                         ) # type: ignore
print(   "111"       ) # type: ignore
