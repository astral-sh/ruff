f = open('foo.txt')  # SIM115
data = f.read()
f.close()

with open('foo.txt') as f:  # OK
    data = f.read()
