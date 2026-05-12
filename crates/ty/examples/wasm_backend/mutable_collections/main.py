values = [1, 2, 3]
labels = ["ty", "wasm"]
scores = {"ty": 1}
names = {"lang": "py"}

values[1] = 9
labels[0] = "ruff"
scores["ty"] = 7
names["lang"] = "wasm"

print(values[1])
print(labels[0])
print(scores["ty"])
print(names["lang"])
