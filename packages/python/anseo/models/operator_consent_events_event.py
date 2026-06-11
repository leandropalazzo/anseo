from enum import Enum


class OperatorConsentEventsEvent(str, Enum):
    OPTIN = "optin"
    OPTOUT = "optout"

    def __str__(self) -> str:
        return str(self.value)
