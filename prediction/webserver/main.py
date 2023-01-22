from flask import Flask, request, jsonify
import pandas as pd
import numpy as np

app = Flask(__name__)
@app.route("/prediction", methods=["get"])
def predict():

    return jsonify({"prediction": 4500})

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=5000)




