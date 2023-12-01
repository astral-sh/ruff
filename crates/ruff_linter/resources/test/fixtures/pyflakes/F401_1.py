"""Access a sub-importation via an alias."""
import pyarrow as pa
import pyarrow.csv

print(pa.csv.read_csv("test.csv"))
