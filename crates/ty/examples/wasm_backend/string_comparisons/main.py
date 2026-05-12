left = "apple"
right = "banana"
score = 0

if left < right:
    score += 1
if left != right:
    score += 10
if right >= "banana":
    score += 100
if left == "apple":
    score += 1000

print(score)
