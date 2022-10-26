for i in range(10):
    print(i)

print(i)  # name no longer defined on Python 3; no warning yet

for i in range(10):  # name not used within the loop; B007
    print(10)

print(i)  # name no longer defined on Python 3; no warning yet


for _ in range(10):  # _ is okay for a throw-away variable
    print(10)


for i in range(10):
    for j in range(10):
        for k in range(10):  # k not used, i and j used transitively
            print(i + j)


def strange_generator():
    for i in range(10):
        for j in range(10):
            for k in range(10):
                for l in range(10):
                    yield i, (j, (k, l))


for i, (j, (k, l)) in strange_generator():  # i, k not used
    print(j, l)
