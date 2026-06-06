from enum import Enum


class ContributionStatusStatus(str, Enum):
    KEK_MISSING = "kek_missing"
    REDACTION_REJECTED = "redaction_rejected"
    SEALED = "sealed"
    SKIPPED_NOT_OPTED_IN = "skipped_not_opted_in"

    def __str__(self) -> str:
        return str(self.value)
