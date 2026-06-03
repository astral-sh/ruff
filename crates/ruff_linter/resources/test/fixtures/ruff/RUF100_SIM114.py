# Valid: SIM114 fires for the elif-elif pair, noqa on the elif line suppresses it.
# RUF100 should *not* fire (the noqa is "used").
if a > 0:
    print("positive")
elif a == 0:  # noqa: SIM114
    print("zero")
elif a == -1:
    print("zero")

# Invalid: bodies differ, SIM114 does not fire, noqa has nothing to suppress.
# RUF100 *should* fire (the noqa is unused).
if a:
    print("a")
# noqa: SIM114
elif b:
    print("b")
