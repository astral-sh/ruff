# These SHOULD throw an error
while True:
    try:
        pass
    finally:
        continue

while True:
    try:
        pass
    finally:
        if True:
            continue

try:
    pass
finally:
    continue

# These should NOT throw an error
while True:
    try:
        pass
    finally:
        break

while True:
    try:
        pass
    except Exception:
        pass
    else:
        continue
