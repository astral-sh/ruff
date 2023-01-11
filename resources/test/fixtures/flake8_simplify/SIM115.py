f = open('foo.txt')  # S508
data = f.read()
f.close()

with open('foo.txt') as f:  # OK
    data = f.read()
