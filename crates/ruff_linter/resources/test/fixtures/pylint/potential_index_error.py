print([1, 2, 3][3])  # PLE0643
print([1, 2, 3][-4])  # PLE0643
print([1, 2, 3][2147483647])  # PLE0643
print([1, 2, 3][-2147483647])  # PLE0643

print([1, 2, 3][2])  # OK
print([1, 2, 3][0])  # OK
print([1, 2, 3][-3])  # OK
print([1, 2, 3][3:])  # OK
print([1, 2, 3][2147483648])  # OK (i32 overflow, ignored)
print([1, 2, 3][-2147483648])  # OK (i32 overflow, ignored)
