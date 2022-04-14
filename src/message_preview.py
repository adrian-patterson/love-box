import textwrap

print("Available fonts: 16, 24, 48, 64")
font = input("Font: ")
message = input("")

if int(font) == 16:
    print(textwrap.wrap(message, width=90, max_lines=30, placeholder="..."))
elif int(font) == 24:
    print(textwrap.wrap(message, width=70, max_lines=16, placeholder="..."))
elif int(font) == 48:
    print(textwrap.wrap(message, width=31, max_lines=8, placeholder="..."))
elif int(font) == 64:
    print(textwrap.wrap(message, width=26, max_lines=6, placeholder="..."))
