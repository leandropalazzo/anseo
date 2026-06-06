from enum import Enum


class ClaimVerdictStatus(str, Enum):
    ACCURATE = "accurate"
    INACCURATE = "inaccurate"
    UNVERIFIABLE = "unverifiable"

    def __str__(self) -> str:
        return str(self.value)
