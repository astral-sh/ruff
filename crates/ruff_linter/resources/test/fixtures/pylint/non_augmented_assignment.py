# Errors
some_string = "some string"
index, a_number, to_multiply, to_divide, to_cube, timeDiffSeconds, flags = (
    0,
    1,
    2,
    3,
    4,
    5,
    0x3,
)
a_list = [1, 2]
some_set = {"elem"}
mat1, mat2 = None, None

some_string = some_string + "a very long end of string"
index = index - 1
a_list = a_list + ["to concat"]
some_set = some_set | {"to concat"}
to_multiply = to_multiply * 5
to_multiply = 5 * to_multiply
to_multiply = to_multiply * to_multiply
to_divide = to_divide / 5
to_divide = to_divide // 5
to_cube = to_cube**3
to_cube = 3**to_cube
to_cube = to_cube**to_cube
timeDiffSeconds = timeDiffSeconds % 60
flags = flags & 0x1
flags = flags | 0x1
flags = flags ^ 0x1
flags = flags << 1
flags = flags >> 1
mat1 = mat1 @ mat2
a_list[1] = a_list[1] + 1

a_list[0:2] = a_list[0:2] * 3
a_list[:2] = a_list[:2] * 3
a_list[1:] = a_list[1:] * 3
a_list[:] = a_list[:] * 3

index = index * (index + 10)


class T:
    def t(self):
        self.a = self.a + 1


obj = T()
obj.a = obj.a + 1

# OK
a_list[0] = a_list[:] * 3
index = a_number = a_number + 1
a_number = index = a_number + 1
index = index * index + 10
some_string = "a very long start to the string" + some_string
