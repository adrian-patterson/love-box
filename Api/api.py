from email.quoprimime import body_check
from flask import Flask, Response, request, jsonify
from flask_cors import CORS
from message import Message
from message_repository import MessageRepository
import textwrap

app = Flask("__main__")
CORS(app)
message_repository = MessageRepository()


@app.route("/")
def index() -> Response:
    return Response(status=200)


@app.route("/message", methods=["POST"])
def message() -> Response:
    body = request.get_json()
    try:
        message_body = Message(
            message=body["message"],
            fontsize=body["fontsize"],
            image=body["image"],
        )

        message_repository.update_message(message_body)
    except KeyError:
        return Response("Invalid JSON body in request.", 400)

    return Response(status=200)


@app.route("/image", methods=["POST"])
def image() -> Response:
    body = request.get_json()
    try:
        message_body = Message(message=body["message"], image=body["image"])

        message_repository.update_message(message_body)
    except KeyError:
        return Response("Invalid JSON body in request.", 400)

    return Response(status=200)


@app.route("/preview", methods=["POST"])
def preview() -> Response:
    body = request.get_json()
    fontsize = body["fontsize"]
    message = body["message"]

    text_wrapped_message = None
    if int(fontsize) == 30:
        text_wrapped_message = textwrap.wrap(
            message, width=58, max_lines=15, placeholder="..."
        )
    elif int(fontsize) == 48:
        text_wrapped_message = textwrap.wrap(
            message, width=35, max_lines=9, placeholder="..."
        )
    elif int(fontsize) == 64:
        text_wrapped_message = textwrap.wrap(
            message, width=26, max_lines=7, placeholder="..."
        )
    elif int(fontsize) == 96:
        text_wrapped_message = textwrap.wrap(
            message, width=16, max_lines=5, placeholder="..."
        )

    if text_wrapped_message is not None:
        newline_joined = "[NEWLINE]".join(text_wrapped_message)
        return jsonify(message=newline_joined)
    else:
        return Response("Invalid font size.", 400)


def start() -> None:
    app.run(host="0.0.0.0", threaded=True, port=8000)


if __name__ == "__main__":
    start()
