from enum import Enum


class SetupStatusClickhouseState(str, Enum):
    DEGRADED = "degraded"
    HEALTHY = "healthy"
    NOT_CONFIGURED = "not_configured"
    UNKNOWN = "unknown"

    def __str__(self) -> str:
        return str(self.value)
