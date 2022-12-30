export const DEFAULT_PYTHON_SOURCE =
  "import os\n" +
  "\n" +
  "# Define a function that takes an integer n and returns the nth number in the Fibonacci\n" +
  "# sequence.\n" +
  "def fibonacci(n):\n" +
  '    """Compute the nth number in the Fibonacci sequence."""\n' +
  "    x = 1\n" +
  "    if n == 0:\n" +
  "        return 0\n" +
  "    elif n == 1:\n" +
  "        return 1\n" +
  "    else:\n" +
  "        return fibonacci(n - 1) + fibonacci(n - 2)\n" +
  "\n" +
  "\n" +
  "# Use a for loop to generate and print the first 10 numbers in the Fibonacci sequence.\n" +
  "for i in range(10):\n" +
  "    print(fibonacci(i))\n" +
  "\n" +
  "# Output:\n" +
  "# 0\n" +
  "# 1\n" +
  "# 1\n" +
  "# 2\n" +
  "# 3\n" +
  "# 5\n" +
  "# 8\n" +
  "# 13\n" +
  "# 21\n" +
  "# 34\n";
