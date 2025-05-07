import os

os.scandir('.')
os.scandir(b'.')

string_path = '.'
os.scandir(string_path)

bytes_path = b'.'
os.scandir(bytes_path)


from pathlib import Path

path_path = Path('.')
os.scandir(path_path)
