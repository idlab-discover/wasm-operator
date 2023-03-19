from flask import Flask, request, jsonify
import pandas as pd
import numpy as np
from datetime import datetime as dt, timezone, timedelta
from statsmodels.tsa.ar_model import AutoReg

app = Flask(__name__)



def dateToRust(date):
## 2023-01-23T13:04:03.182471956Z
    return date.strftime("%Y-%m-%dT%H:%M:%S.%fZ")


def predictAutoReg(diff):

    # if not enough data/lags take  min
    lags  = min(len(diff)//2,10)
    model = AutoReg(diff, lags=lags).fit()
    predction = model.predict(start=len(diff), end=len(diff), dynamic=False)[0]
    return predction
    




@app.route("/prediction", methods=["post"])
def predict():
    #print("post request made")
    #print(request.json, flush=True)
    history = request.json['history']
    #history = ['2023-03-18T18:28:13.783525711Z', '2023-03-18T18:28:14.253025485Z', '2023-03-18T18:28:14.253166198Z', '2023-03-18T18:28:14.264608495Z']
    
    ## not enough data just return 3 secs
    if  len(history) < 2:
        now = dt.now(timezone.utc)
        now += timedelta(seconds=3)
        now = dateToRust(now)
        return jsonify({"prediction": now})


    dates  = [dt.strptime(date[:26], '%Y-%m-%dT%H:%M:%S.%f') for date in history]

    lastEvent  = dates[-1]
    diff  =  [(dates[i] - dates[i-1]).total_seconds() for i in range(1,len(dates))]
    print(diff,flush=True)
    prediction = 0
    try:
        prediction = predictAutoReg(diff)
        
    except:
        print("error in model")
        prediction= np.mean(diff)

    now = lastEvent
    now += timedelta(seconds=prediction)
    now = dateToRust(now)
    print(f"prediction is  {prediction} and  hist  {diff}",flush=True)
    
    return jsonify({"prediction": now})

if __name__ == "__main__":

    app.run(host="0.0.0.0", port=5000)

