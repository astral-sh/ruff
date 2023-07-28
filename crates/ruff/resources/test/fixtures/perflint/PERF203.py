# PERF203
for i in range(10):
    try:
        print(f"{i}")
    except:
        print("error")

# OK
try:
    for i in range(10):
        print(f"{i}")
except:
    print("error")

# OK
i = 0
while i < 10:
    try:
        print(f"{i}")
    except:
        print("error")

    i += 1
