print([1, 2, 3][3])  # PLE0643
print([1, 2, 3][-4])  # PLE0643
print([1, 2, 3][999999999999999999999999999999999999999999])  # PLE0643
print([1, 2, 3][-999999999999999999999999999999999999999999])  # PLE0643

print([1, 2, 3][2])  # OK
print([1, 2, 3][0])  # OK
print([1, 2, 3][-3])  # OK
print([1, 2, 3][3:])  # OK

# Starred elements (runtime length cannot be statically known)
a = (1, 2, 3)
print((*a, 9)[2])  # OK
print([*a, *a][2])  # OK
print((*a,)[5])  # OK
print([*a, 1][-10])  # OK

