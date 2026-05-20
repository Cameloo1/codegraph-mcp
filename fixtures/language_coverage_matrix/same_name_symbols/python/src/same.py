def duplicate(value):
    return value + 1

class Box:
    def duplicate(self):
        return 2

def caller():
    return duplicate(1)
