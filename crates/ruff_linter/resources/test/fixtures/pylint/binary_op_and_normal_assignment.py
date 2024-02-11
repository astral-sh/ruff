some_string = "some string" # PLR6104
index, a_number, to_multiply, to_divide, to_cube, timeDiffSeconds, flags = 0, 1, 2, 3, 4, 5, 0x3 # PLR6104
a_list = [1,2] # PLR6104
some_set = {"elem"} # PLR6104
mat1, mat2 = None, None # PLR6104

some_string = (
  some_string
  + "a very long end of string"
) # PLR6104
index = index - 1 # PLR6104
a_list = a_list + ["to concat"] # PLR6104
some_set = some_set | {"to concat"} # PLR6104
to_multiply = to_multiply * 5 # PLR6104
to_divide = to_divide / 5 # PLR6104
to_divide = to_divide // 5 # PLR6104
to_cube = to_cube ** 3 # PLR6104
timeDiffSeconds = timeDiffSeconds % 60 # PLR6104
flags = flags & 0x1 # PLR6104
flags = flags | 0x1 # PLR6104
flags = flags ^ 0x1 # PLR6104
flags = flags << 1 # PLR6104
flags = flags >> 1 # PLR6104
mat1 = mat1 @ mat2 # PLR6104
a_list[1] = a_list[1] + 1 # PLR6104

a_list[0:2] = a_list[0:2] * 3 # PLR6104
a_list[:2] = a_list[:2] * 3 # PLR6104
a_list[1:] = a_list[1:] * 3 # PLR6104
a_list[:] = a_list[:] * 3 # PLR6104

index = index * (index + 10)  # PLR6104

class T:
    def t(self):
        self.a = self.a + 1 # PLR6104

obj = T()
obj.a = obj.a + 1 # PLR6104

a_list[0] = a_list[:] * 3 # OK
index = a_number = a_number + 1 # OK
a_number = index = a_number + 1 # OK
index = index * index + 10 # OK