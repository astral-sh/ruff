fruits = ["apples", "plums", "pear"]
fruits.filter(lambda fruit: fruit.startwith("p"))
assert len(fruits), 2

assert True, "always true"