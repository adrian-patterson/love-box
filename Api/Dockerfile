FROM ubuntu:latest
RUN apt update -y && apt upgrade -y

FROM python:latest

LABEL Maintainer="canadrian72"

WORKDIR /app

RUN pip3 install flask requests flask-cors python-dateutil firebase-admin

COPY api.py message.py message_repository.py private_key.json ./

ENV PYTHONPATH "${PYTHONPATH}:/app/"

EXPOSE 8000
CMD [ "python", "-u", "api.py"]