from enum import Enum

class PluginStatusKind(str, Enum):
    ANALYTICS = "analytics"
    EXTRACTOR = "extractor"
    OUTPUT_FORMAT = "output-format"
    PROVIDER = "provider"
    UNKNOWN = "unknown"

    def __str__(self) -> str:
        return str(self.value)
