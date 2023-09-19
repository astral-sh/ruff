books = ["Dune", "Foundation", "Neuromancer"]

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

for index, (_, _) in enumerate(books):
    print(index)

for (_, _), book in enumerate(books):
    print(book)

for(index, _)in enumerate(books):
    print(index)

for(index), _ in enumerate(books):
    print(index)

# OK
for index, book in enumerate(books):
    print(index, book)

for index in range(len(books)):
    print(index)

for book in books:
    print(book)
