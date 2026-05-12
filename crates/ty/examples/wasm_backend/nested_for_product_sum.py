weights = [1, 2]
total = 0

for left in range(3):
    for weight in weights:
        total += left * weight

print(total)
