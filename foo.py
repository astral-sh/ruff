import timeit

def function_1():
    # Function 1 code here
    pass

def function_2():
    # Function 2 code here
    pass

# Run the benchmark
num_runs = 1000
function_1_time = timeit.timeit(function_1, number=num_runs)
function_2_time = timeit.timeit(function_2, number=num_runs)

# Print the results
print(f"Function 1 average time: {function_1_time / num_runs:.6f} seconds")
print(f"Function 2 average time: {function_2_time / num_runs:.6f} seconds")
