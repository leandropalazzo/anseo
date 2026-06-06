from enum import Enum


class CrawlerIngestRequestFormat(str, Enum):
    COMBINED = "combined"
    COMMON = "common"

    def __str__(self) -> str:
        return str(self.value)
