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
        test = "aa"
        match test:
            case "aa":
                continue  # [continue-in-finally]

while True:
    try:
        pass
    finally:
        with "aa" as f:
            continue  # [continue-in-finally]

while True:
    try:
        pass
    finally:
        if True:
            continue  # [continue-in-finally]
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

        while True:
            pass
        else:
            continue  # [continue-in-finally]

        def test():
            continue
            while True:
                continue


while True:
    try:
        pass
    finally:
        if True:
            pass
        elif False:
            continue  # [continue-in-finally]
        else:
            continue  # [continue-in-finally]
            for i in range(10):
                pass
            else:
                continue  # [continue-in-finally]
