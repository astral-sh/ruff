from flask import Flask

app = Flask(__name__)

@app.route('/')
def main():
    raise

#bad
app.run(debug=True)

#okay
app.run()
app.run(debug=False)

#unrelated
run()
run(debug=True)
run(debug)
foo.run(debug=True)
app = 1
app.run(debug=True)
