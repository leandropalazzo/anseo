from enum import Enum

class CrawlerIngestRequestPrivacyMode(str, Enum):
    HASHED = "hashed"
    RAW = "raw"
    TRUNCATED = "truncated"

    def __str__(self) -> str:
        return str(self.value)
