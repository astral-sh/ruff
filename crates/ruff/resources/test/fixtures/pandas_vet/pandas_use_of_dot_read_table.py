import pandas as pd

# Errors.

df = pd.read_table("data.csv", sep=",")

# Non-errors.

df = pd.read_csv("data.csv")
df = pd.read_table("data.tsv")
