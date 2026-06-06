from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from dateutil.parser import isoparse
from typing import cast
import datetime

if TYPE_CHECKING:
  from ..models.contribution_status import ContributionStatus





T = TypeVar("T", bound="IngestRunResponse")



@_attrs_define
class IngestRunResponse:
    """ Result of recording an external run, including the benchmark contribution outcome.

        Attributes:
            run_id (str):
            project_id (str):
            prompt_slug (str):
            provider (str):
            observed_at (datetime.datetime):
            contribution (ContributionStatus): Outcome of the benchmark consent + envelope gate. `sealed`: opted in with a
                KEK, redacted + sealed. `skipped_not_opted_in`: no active opt-in. `kek_missing`: opted in but no per-project KEK
                available, so the contribution could NOT be sealed (the run is still recorded; benchmark data is flagged, never
                silently dropped). `redaction_rejected`: redaction refused the run (e.g. stale consent terms).
     """

    run_id: str
    project_id: str
    prompt_slug: str
    provider: str
    observed_at: datetime.datetime
    contribution: ContributionStatus
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.contribution_status import ContributionStatus
        run_id = self.run_id

        project_id = self.project_id

        prompt_slug = self.prompt_slug

        provider = self.provider

        observed_at = self.observed_at.isoformat()

        contribution = self.contribution.to_dict()


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "run_id": run_id,
            "project_id": project_id,
            "prompt_slug": prompt_slug,
            "provider": provider,
            "observed_at": observed_at,
            "contribution": contribution,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.contribution_status import ContributionStatus
        d = dict(src_dict)
        run_id = d.pop("run_id")

        project_id = d.pop("project_id")

        prompt_slug = d.pop("prompt_slug")

        provider = d.pop("provider")

        observed_at = isoparse(d.pop("observed_at"))




        contribution = ContributionStatus.from_dict(d.pop("contribution"))




        ingest_run_response = cls(
            run_id=run_id,
            project_id=project_id,
            prompt_slug=prompt_slug,
            provider=provider,
            observed_at=observed_at,
            contribution=contribution,
        )


        ingest_run_response.additional_properties = d
        return ingest_run_response

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
