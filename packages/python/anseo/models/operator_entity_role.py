from enum import Enum


class OperatorEntityRole(str, Enum):
    BOTH = "both"
    BRAND = "brand"
    SOURCE = "source"

    def __str__(self) -> str:
        return str(self.value)
