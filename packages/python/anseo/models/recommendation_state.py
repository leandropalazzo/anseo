from enum import Enum


class RecommendationState(str, Enum):
    ACKNOWLEDGED = "acknowledged"
    ACTED = "acted"
    DISMISSED = "dismissed"
    GENERATED = "generated"
    MEASURED = "measured"
    STALE = "stale"
    SURFACED = "surfaced"

    def __str__(self) -> str:
        return str(self.value)
