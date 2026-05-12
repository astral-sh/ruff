scores = {"ty": 9, "wasm": 4}
key_chars = 0
total = 0

for key in scores:
    key_chars += len(key)
    total += scores[key]

print(key_chars)
print(total)
