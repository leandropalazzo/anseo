from enum import Enum


class OperatorListEntitiesVerificationMethod(str, Enum):
    DNS_TXT = "dns_txt"
    EMAIL_MAGIC_LINK = "email_magic_link"
    MANUAL_OVERRIDE = "manual_override"

    def __str__(self) -> str:
        return str(self.value)
