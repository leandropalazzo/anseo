from enum import Enum


class TransitionRecommendationRequestTo(str, Enum):
    ACKNOWLEDGED = "acknowledged"
    ACTED = "acted"
    DISMISSED = "dismissed"
    MEASURED = "measured"
    STALE = "stale"
    SURFACED = "surfaced"

    def __str__(self) -> str:
        return str(self.value)
