# Errors
a = int(input())
b = int(input())
c = int(input())
if a < b and b < c:
    pass

while a < b and b < c:
    pass

x = int(input())
y = int(input())
z = int(input())
if x <= y and y < z:
    print("In mixed order")

while x <= y and y < z:
    print("In mixed order")

i = int(input())
j = int(input())
k = int(input())
if i > j and j > k:
    print("Descending order")

while i > j and j > k:
    print("Descending order")

# OK
a = int(input())
b = int(input())
c = int(input())
if a < b < c:
    pass

while a < b < c:
    pass

x = int(input())
y = int(input())
z = int(input())
if x <= y < z:
    print("In mixed order")

while x <= y < z:
    print("In mixed order")

i = int(input())
j = int(input())
k = int(input())
if i > j > k:
    print("Descending order")

while i > j > k:
    print("Descending order")
