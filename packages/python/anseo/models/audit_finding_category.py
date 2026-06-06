from enum import Enum


class AuditFindingCategory(str, Enum):
    CORROBORATION = "corroboration"
    EXTRACTABILITY = "extractability"
    IDENTITY = "identity"

    def __str__(self) -> str:
        return str(self.value)
