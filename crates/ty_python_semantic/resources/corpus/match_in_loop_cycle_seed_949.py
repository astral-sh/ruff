# Regression test for a cycle found with py-fuzzer seed 949.

values = []
value = 0
for _outer in 0:
    match (value for _inner in values):
        case Color.VALUE as value if 0:
            pass
        case Pattern() if value:
            type value = int
        case _ if guard:
            break
