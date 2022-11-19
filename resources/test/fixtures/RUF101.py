exit(1)


def main():
    exit(6)  # Is not detected


import sys

exit(0)


def main():
    exit(3)  # Is not detected


sys.exit(7)
del sys


import sys as sys2

exit(2)


def main():
    exit(9)  # Is not detected


del sys2


from sys import exit as exit2

exit(5)


def main():
    exit(10)  # Is not detected


def exit(e):
    pass


exit(8)
