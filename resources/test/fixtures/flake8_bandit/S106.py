def func(pos, password):
    pass


string = "Hello World"

# OK
func("s3cr3t")
func(1, password=string)
func(1, password="")
func(pos="s3cr3t", password=string)

# Error
func(1, password="s3cr3t")
