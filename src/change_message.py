import os
from repository.message_repository import MessageRepository
from domain.message import Message


def update_db(message: Message) -> None:
    message_controller = MessageRepository()
    message_controller.update_message(message)


if __name__ == "__main__":
    choice = int(input("Message Type:\n\t(1) Message\n\t(2) Image\nChoice:\t "))
    fonts = [16, 24, 48, 64]

    if choice == 1:
        print("Available fonts: 16, 24, 48, 64")
        fontsize = input("Font Size: ")
        message = input("New Message: ")

        if int(fontsize) not in fonts:
            print("Invalid font size.")
        else:
            new_message = Message(message, fontsize)
            update_db(new_message)
    elif choice == 2:
        print("\nAvailable Images: ")
        pictures = os.listdir("../pic")

        image_count = 0
        image_dict = {}
        for file in pictures:
            file_name, file_type = file.split(".")[0], file.split(".")[1]
            if file_type == "bmp":
                image_count += 1
                print("(" + str(image_count) + ") \t" + file_name)
                image_dict[image_count] = file
        new_image = Message(message=image_dict[int(input("\nChoice: "))], image=True)
        update_db(new_image)
