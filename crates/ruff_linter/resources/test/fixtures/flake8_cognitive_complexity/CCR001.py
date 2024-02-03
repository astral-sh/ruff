# Cognitive Complexity 7
def sumOfPrimes(max: int):
    total = 0
    for i in range(max):  # +1
        for j in range(2, i):  # +2
            if i % j == 0:  # +3
                continue  # +1
        total += i
    return total
