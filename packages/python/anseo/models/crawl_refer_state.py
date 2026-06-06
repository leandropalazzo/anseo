from enum import Enum


class CrawlReferState(str, Enum):
    COMPLETE = "complete"
    CRAWLS_ONLY = "crawls_only"

    def __str__(self) -> str:
        return str(self.value)
