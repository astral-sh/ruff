# Cognitive Complexity 7
def sumOfPrimes(max: int):
    total = 0
    for i in range(max):  # +1 (nested 0)
        for j in range(2, i):  # +2 (nested 1)
            if i % j == 0:  # +3 (nested 2)
                continue  # +1 (nested N/A)
        total += i
    return total
