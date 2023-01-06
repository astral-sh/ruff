if a:  # SIM102
    if b:
        c


if a:
    pass
elif b:  # SIM102
    if c:
        d

if a:
    if b:
        c
    else:
        d

if __name__ == "__main__":
    if foo():
        ...

if a:
    d
    if b:
        c
