def foo():
    with open("file") as f, bar("") as ((a, b)):
        print("hello")
