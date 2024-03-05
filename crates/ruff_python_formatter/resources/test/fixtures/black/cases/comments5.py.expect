while True:
    if something.changed:
        do.stuff()  # trailing comment
        # Comment belongs to the `if` block.
    # This one belongs to the `while` block.

    # Should this one, too?  I guess so.

# This one is properly standalone now.

for i in range(100):
    # first we do this
    if i % 33 == 0:
        break

    # then we do this
    print(i)
    # and finally we loop around

with open(some_temp_file) as f:
    data = f.read()

try:
    with open(some_other_file) as w:
        w.write(data)

except OSError:
    print("problems")

import sys


# leading function comment
def wat():
    ...
    # trailing function comment


# SECTION COMMENT


# leading 1
@deco1
# leading 2
@deco2(with_args=True)
# leading 3
@deco3
def decorated1(): ...


# leading 1
@deco1
# leading 2
@deco2(with_args=True)
# leading function comment
def decorated1(): ...


# Note: this is fixed in
# Preview.empty_lines_before_class_or_def_with_leading_comments.
# In the current style, the user will have to split those lines by hand.
some_instruction


# This comment should be split from `some_instruction` by two lines but isn't.
def g(): ...


if __name__ == "__main__":
    main()
