pair: tuple[str, str] = ("ty", "wasm")
chars = 0
for word in pair:
    chars += len(word)

print(pair[1])
print(len(pair))
print(chars)
