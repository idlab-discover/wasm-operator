from flask import Flask, request, jsonify
import pandas as pd
import numpy as np
from datetime import datetime as dt, timezone, timedelta

app = Flask(__name__)



def dateToRust(date):
## 2023-01-23T13:04:03.182471956Z
    return date.strftime("%Y-%m-%dT%H:%M:%S.%fZ")



@app.route("/prediction", methods=["post"])
def predict():
    print(request.json)
    now = dt.now(timezone.utc)
    now += timedelta(seconds=3)
    now = dateToRust(now)
    return jsonify({"prediction": now})

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=5000)

