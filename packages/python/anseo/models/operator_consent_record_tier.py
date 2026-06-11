from enum import Enum


class OperatorConsentRecordTier(str, Enum):
    ANONYMOUS = "anonymous"
    BRAND_VISIBILITY = "brand_visibility"

    def __str__(self) -> str:
        return str(self.value)
