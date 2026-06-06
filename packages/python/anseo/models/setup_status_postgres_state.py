from enum import Enum


class SetupStatusPostgresState(str, Enum):
    DEGRADED = "degraded"
    HEALTHY = "healthy"
    UNKNOWN = "unknown"

    def __str__(self) -> str:
        return str(self.value)
