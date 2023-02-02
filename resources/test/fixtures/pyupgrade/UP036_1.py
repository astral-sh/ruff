import sys

if sys.version_info == 2:
    2
else:
    3

if sys.version_info < (3,):
    2
else:
    3

if sys.version_info < (3,0):
    2
else:
    3

if sys.version_info == 3:
    3
else:
    2

if sys.version_info > (3,):
    3
else:
    2

if sys.version_info >= (3,):
    3
else:
    2

from sys import version_info

if version_info > (3,):
    3
else:
    2

if True:
    print(1)
elif sys.version_info < (3,0):
    print(2)
else:
    print(3)

if True:
    print(1)
elif sys.version_info > (3,):
    print(3)
else:
    print(2)

if True:
    print(1)
elif sys.version_info > (3,):
    print(3)

def f():
    if True:
        print(1)
    elif sys.version_info > (3,):
        print(3)

if True:
    print(1)
elif sys.version_info < (3,0):
    print(2)
else:
    print(3)

def f():
    if True:
        print(1)
    elif sys.version_info > (3,):
        print(3)
