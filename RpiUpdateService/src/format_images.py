from PIL import Image, ImageOps
from os.path import dirname, join, realpath, exists
from os import remove, chdir, listdir
import json


def resize():
    image_path = join(
        dirname(dirname(dirname(realpath(__file__)))), "LoveBoxUi/public/images/"
    )

    for file in listdir():
        file_name, extension = file.split(".")
        if extension.lower() == "jpeg" or extension.lower() == "jpg":
            print("Resizing " + file)
            image = Image.open(file)
            resized_image = ImageOps.fit(image, (800, 480), Image.ANTIALIAS)

            if exists(file_name + ".bmp"):
                i = 1
                while exists(file_name + "_" + str(i) + ".bmp"):
                    i += 1
                file_name = file_name + "_" + str(i)

            resized_image.save(file_name + ".bmp", "BMP", quality=95)
            resized_image.save(image_path + file_name + ".bmp", "BMP", quality=95)
            remove(file)


class Photo:
    def __init__(self, src, key, width, height):
        self.src: str = src
        self.key: str = key
        self.width: float = width
        self.height: float = height


def create_json_file():
    photos_json_path = join(
        dirname(dirname(dirname(realpath(__file__)))), "LoveBoxUi/src/images/"
    )

    photo_json_list: list[str] = []
    photo_json_list.append("export const photos = [")
    for file in listdir():
        if file.split(".")[1] == "bmp":
            photo = Photo("/images/" + file, file, 8, 4.8)
            photo_json_list.append(json.dumps(photo.__dict__) + ",")
    photo_json_list.append("];")

    with open(photos_json_path + "photos.ts", "w+") as json_output_file:
        json_output_file.writelines(photo_json_list)


if __name__ == "__main__":
    chdir("pic")
    resize()
    create_json_file()
