from enum import Enum

class ComparisonsWindow(str, Enum):
    VALUE_0 = "1d"
    VALUE_1 = "7d"
    VALUE_2 = "30d"

    def __str__(self) -> str:
        return str(self.value)
