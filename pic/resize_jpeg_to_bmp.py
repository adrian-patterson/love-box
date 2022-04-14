from PIL import Image
import os

def resize():
    for file in os.listdir():
        file_name, extension = file.split('.')
        if extension == 'jpeg' or extension == 'jpg':
            print("Resizing " + file)
            im = Image.open(file)
            resized_image = im.resize((800,480), Image.ANTIALIAS)
            resized_image.save(file_name + '.bmp', 'BMP', quality=95)
            os.remove(file)

resize()