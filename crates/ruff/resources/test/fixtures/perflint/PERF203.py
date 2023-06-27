for i in range(10):
    try:  # PERF203
        print(f"{i}")
    except:
        print("error")

try:
    for i in range(10):
        print(f"{i}")
except:
    print("error")

i = 0
while i < 10:  # PERF203
    try:
        print(f"{i}")
    except:
        print("error")

    i += 1

try:
    i = 0
    while i < 10:
        print(f"{i}")
        i += 1
except:
    print("error")
