from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.recommendation_state import RecommendationState

if TYPE_CHECKING:
    from ..models.recommendation_payload import RecommendationPayload
    from ..models.recommendation_reproducibility import RecommendationReproducibility
    from ..models.recommendation_traceability import RecommendationTraceability


T = TypeVar("T", bound="Recommendation")


@_attrs_define
class Recommendation:
    """Story 19.6 — a stored GEO Recommendation (architecture-phase3-geo-recommendations.md §8 wire shape) plus its DB
    lifecycle `state`.

        Attributes:
            confidence_band (str):
            engine_version (str):
            generated_at (datetime.datetime):
            id (str): ULID.
            kind (str):
            payload (RecommendationPayload):
            project_id (str): ULID.
            reproducibility (RecommendationReproducibility):
            severity (str):
            state (RecommendationState):
            summary (str):
            tags (list[str]):
            traceability (RecommendationTraceability):
    """

    confidence_band: str
    engine_version: str
    generated_at: datetime.datetime
    id: str
    kind: str
    payload: RecommendationPayload
    project_id: str
    reproducibility: RecommendationReproducibility
    severity: str
    state: RecommendationState
    summary: str
    tags: list[str]
    traceability: RecommendationTraceability
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        confidence_band = self.confidence_band

        engine_version = self.engine_version

        generated_at = self.generated_at.isoformat()

        id = self.id

        kind = self.kind

        payload = self.payload.to_dict()

        project_id = self.project_id

        reproducibility = self.reproducibility.to_dict()

        severity = self.severity

        state = self.state.value

        summary = self.summary

        tags = self.tags

        traceability = self.traceability.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "confidence_band": confidence_band,
                "engine_version": engine_version,
                "generated_at": generated_at,
                "id": id,
                "kind": kind,
                "payload": payload,
                "project_id": project_id,
                "reproducibility": reproducibility,
                "severity": severity,
                "state": state,
                "summary": summary,
                "tags": tags,
                "traceability": traceability,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.recommendation_payload import RecommendationPayload
        from ..models.recommendation_reproducibility import (
            RecommendationReproducibility,
        )
        from ..models.recommendation_traceability import RecommendationTraceability

        d = dict(src_dict)
        confidence_band = d.pop("confidence_band")

        engine_version = d.pop("engine_version")

        generated_at = datetime.datetime.fromisoformat(d.pop("generated_at"))

        id = d.pop("id")

        kind = d.pop("kind")

        payload = RecommendationPayload.from_dict(d.pop("payload"))

        project_id = d.pop("project_id")

        reproducibility = RecommendationReproducibility.from_dict(
            d.pop("reproducibility")
        )

        severity = d.pop("severity")

        state = RecommendationState(d.pop("state"))

        summary = d.pop("summary")

        tags = cast(list[str], d.pop("tags"))

        traceability = RecommendationTraceability.from_dict(d.pop("traceability"))

        recommendation = cls(
            confidence_band=confidence_band,
            engine_version=engine_version,
            generated_at=generated_at,
            id=id,
            kind=kind,
            payload=payload,
            project_id=project_id,
            reproducibility=reproducibility,
            severity=severity,
            state=state,
            summary=summary,
            tags=tags,
            traceability=traceability,
        )

        recommendation.additional_properties = d
        return recommendation

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
