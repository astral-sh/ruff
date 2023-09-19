from flask import Flask

app = Flask(__name__)

@app.route('/')
def main():
    raise

# OK
app.run(debug=True)

# Errors
app.run()
app.run(debug=False)

# Unrelated
run()
run(debug=True)
run(debug)
foo.run(debug=True)
app = 1
app.run(debug=True)
