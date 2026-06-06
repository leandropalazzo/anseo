from enum import Enum


class SetupStatusWorkerState(str, Enum):
    RUNNING = "running"
    STOPPED = "stopped"
    UNKNOWN = "unknown"

    def __str__(self) -> str:
        return str(self.value)
