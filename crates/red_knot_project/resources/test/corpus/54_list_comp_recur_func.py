def recur1(a):
    return [recur1(b) for b in a]
