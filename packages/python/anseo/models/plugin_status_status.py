from enum import Enum


class PluginStatusStatus(str, Enum):
    LOADED = "loaded"
    LOAD_ERROR = "load_error"
    SKIPPED = "skipped"

    def __str__(self) -> str:
        return str(self.value)
