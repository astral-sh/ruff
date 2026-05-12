total: int = 0
value: int = 1
running: bool = value <= 10

while running:
    total += value
    value += 1
    running = value <= 10

print(total)
