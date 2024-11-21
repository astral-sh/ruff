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
