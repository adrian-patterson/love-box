import os
import firebase_admin
from firebase_admin import credentials, db
from message import Message
from datetime import datetime


class MessageRepository:
    def __init__(self) -> None:
        cred = credentials.Certificate(os.environ["FIREBASE_PRIVATE_KEY"])
        firebase_admin.initialize_app(
            cred,
            {"databaseURL": "https://emilymessageboard-default-rtdb.firebaseio.com/"},
        )
        self.ref = db.reference("/")
        self.message = Message(**self.ref.get())

    def get_message_string(self) -> str:
        return self.message.message

    def get_font_size(self) -> int:
        return self.message.fontsize

    def get_datetime_last_modified(self) -> datetime:
        return datetime.fromisoformat(self.message.modified)

    def is_image(self) -> bool:
        return self.message.image

    def update_message(self, message: Message) -> None:
        self.ref.update(message.__dict__)
