import argparse
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("target_dir", type=Path)
args = parser.parse_args()
parser.error(f"Target directory {args.target_dir} does not exist")
