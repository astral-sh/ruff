if not (not a):  # SIM208
    pass

if not (not (a == b)):  # SIM208
    pass

if not a:  # OK
    pass

if not a == b:  # OK
    pass

if not a != b:  # OK
    pass

a = not not b  # SIM208

f(not not a)  # SIM208

if 1 + (not (not a)):  # SIM208
    pass

print(not not x in y)  # SIM208
print(not not x not in y)  # SIM208
print(not not x is y)  # SIM208
print(not not x is not y)  # SIM208
print(not not bool(x))  # SIM208

