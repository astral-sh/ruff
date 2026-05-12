words = ["ty"]
words.append("wasm")

labels = {"tool": "ty", "target": "wasm"}
chars = 0
for word in words:
    chars += len(word)

print(words[1])
print(labels["target"])
print(chars)
