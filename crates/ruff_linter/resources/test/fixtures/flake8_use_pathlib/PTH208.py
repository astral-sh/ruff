import os

os.listdir('.')
os.listdir(b'.')

string_path = '.'
os.listdir(string_path)

bytes_path = b'.'
os.listdir(bytes_path)


from pathlib import Path

path_path = Path('.')
os.listdir(path_path)


if os.listdir("dir"):
    ...

if "file" in os.listdir("dir"):
    ...

os.listdir(1)
os.listdir(path=1)
