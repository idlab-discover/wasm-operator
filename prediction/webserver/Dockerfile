FROM python:3.10-slim

WORKDIR /app
COPY requirements.txt /app/requirements.txt
COPY main.py /app/main.py

RUN pip install -r requirements.txt 

EXPOSE 5000
CMD [ "python", "main.py" ]