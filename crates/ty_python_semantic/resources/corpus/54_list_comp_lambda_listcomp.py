def f():
    [(lambda a:[a**i for i in range(a+1)])(j) for j in range(5)]
