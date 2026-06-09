from enum import Enum


class OperatorEntityClaimStatus(str, Enum):
    PENDING = "pending"
    PENDING_CONFLICT = "pending_conflict"
    REVOKED = "revoked"
    UNCLAIMED = "unclaimed"
    VERIFIED = "verified"

    def __str__(self) -> str:
        return str(self.value)
