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

# OK - no other way to write this
for i in range(10):
    try:
        print(f"{i}")
        break
    except:
        print("error")

# OK - no other way to write this
for i in range(10):
    try:
        print(f"{i}")
        continue
    except:
        print("error")


# OK - no other way to write this
for i in range(10):
    try:
        print(f"{i}")
        if i > 0:
            break
    except:
        print("error")
