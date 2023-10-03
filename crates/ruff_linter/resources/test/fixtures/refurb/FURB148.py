books = ["Dune", "Foundation", "Neuromancer"]

books_and_authors = {
    "Dune": "Frank Herbert",
    "Foundation": "Isaac Asimov",
    "Neuromancer": "William Gibson",
}

books_set = {"Dune", "Foundation", "Neuromancer"}

books_tuple = ("Dune", "Foundation", "Neuromancer")

# Errors
for index, _ in enumerate(books):
    print(index)

for index, _ in enumerate(books, start=0):
    print(index)

for index, _ in enumerate(books, 0):
    print(index)

for index, _ in enumerate(books, start=1):
    print(index)

for index, _ in enumerate(books, 1):
    print(index)

for index, _ in enumerate(books, start=x):
    print(book)

for index, _ in enumerate(books, x):
    print(book)

for _, book in enumerate(books):
    print(book)

for _, book in enumerate(books, start=0):
    print(book)

for _, book in enumerate(books, 0):
    print(book)

for _, book in enumerate(books, start=1):
    print(book)

for _, book in enumerate(books, 1):
    print(book)

for _, book in enumerate(books, start=x):
    print(book)

for _, book in enumerate(books, x):
    print(book)

for index, (_, _) in enumerate(books):
    print(index)

for (_, _), book in enumerate(books):
    print(book)

for(index, _)in enumerate(books):
    print(index)

for(index), _ in enumerate(books):
    print(index)

for index, _ in enumerate(books_and_authors):
    print(index)

for _, book in enumerate(books_and_authors):
    print(book)

for index, _ in enumerate(books_set):
    print(index)

for _, book in enumerate(books_set):
    print(book)

for index, _ in enumerate(books_tuple):
    print(index)

for _, book in enumerate(books_tuple):
    print(book)

# OK
for index, book in enumerate(books):
    print(index, book)

for index in range(len(books)):
    print(index)

for book in books:
    print(book)

# Generators don't support the len() function.
# https://github.com/astral-sh/ruff/issues/7656
a = (b for b in range(1, 100))
for i, _ in enumerate(a):
    print(i)
