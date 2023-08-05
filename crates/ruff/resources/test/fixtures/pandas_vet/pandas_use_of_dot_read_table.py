import pandas as pd

# Errors.
df = pd.read_table("data.csv", sep=",")
df = pd.read_table("data.csv", sep=",", header=0)
filename = "data.csv"
df = pd.read_table(filename, sep=",")
df = pd.read_table(filename, sep=",", header=0)

# Non-errors.
df = pd.read_csv("data.csv")
df = pd.read_table("data.tsv")
df = pd.read_table("data.tsv", sep="\t")
df = pd.read_table("data.tsv", sep=",,")
df = pd.read_table("data.tsv", sep=", ")
df = pd.read_table("data.tsv", sep=" ,")
df = pd.read_table("data.tsv", sep=" , ")
not_pd.read_table("data.csv", sep=",")
data = read_table("data.csv", sep=",")
data = read_table
