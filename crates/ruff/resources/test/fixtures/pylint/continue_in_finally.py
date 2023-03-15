while True:
    try:
        pass
    finally:
        continue  # [continue-in-finally]

while True:
    try:
        pass
    except Exception:
        continue
    finally:
        try:
            pass
        finally:
            continue  # [continue-in-finally]
        pass


while True:
    try:
        pass
    finally:
        continue  # [continue-in-finally]

        def test():
            while True:
                continue
            try:
                pass
            finally:
                continue  # [continue-in-finally]


while True:
    try:
        pass
    finally:
        continue  # [continue-in-finally]

        def test():
            while True:
                continue


while True:
    try:
        pass
    finally:
        for i in range(12):
            continue
        continue  # [continue-in-finally]

        def test():
            continue
            while True:
                continue
