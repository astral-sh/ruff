result: int = 1
value: int = 1
running: bool = value <= 6

while running:
    result *= value
    value += 1
    running = value <= 6

print(result)
