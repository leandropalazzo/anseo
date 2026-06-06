from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.claim_verdict_status import ClaimVerdictStatus
from ..types import UNSET, Unset
from dateutil.parser import isoparse
from typing import cast
import datetime






T = TypeVar("T", bound="ClaimVerdict")



@_attrs_define
class ClaimVerdict:
    """ 
        Attributes:
            entity (str):
            claim_text (str):
            claim_kind (str):
            status (ClaimVerdictStatus):
            rationale (str):
            prompt_run_id (str):
            observed_at (datetime.datetime):
            matched_fact_key (None | str | Unset):
     """

    entity: str
    claim_text: str
    claim_kind: str
    status: ClaimVerdictStatus
    rationale: str
    prompt_run_id: str
    observed_at: datetime.datetime
    matched_fact_key: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        entity = self.entity

        claim_text = self.claim_text

        claim_kind = self.claim_kind

        status = self.status.value

        rationale = self.rationale

        prompt_run_id = self.prompt_run_id

        observed_at = self.observed_at.isoformat()

        matched_fact_key: None | str | Unset
        if isinstance(self.matched_fact_key, Unset):
            matched_fact_key = UNSET
        else:
            matched_fact_key = self.matched_fact_key


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "entity": entity,
            "claim_text": claim_text,
            "claim_kind": claim_kind,
            "status": status,
            "rationale": rationale,
            "prompt_run_id": prompt_run_id,
            "observed_at": observed_at,
        })
        if matched_fact_key is not UNSET:
            field_dict["matched_fact_key"] = matched_fact_key

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        entity = d.pop("entity")

        claim_text = d.pop("claim_text")

        claim_kind = d.pop("claim_kind")

        status = ClaimVerdictStatus(d.pop("status"))




        rationale = d.pop("rationale")

        prompt_run_id = d.pop("prompt_run_id")

        observed_at = isoparse(d.pop("observed_at"))




        def _parse_matched_fact_key(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        matched_fact_key = _parse_matched_fact_key(d.pop("matched_fact_key", UNSET))


        claim_verdict = cls(
            entity=entity,
            claim_text=claim_text,
            claim_kind=claim_kind,
            status=status,
            rationale=rationale,
            prompt_run_id=prompt_run_id,
            observed_at=observed_at,
            matched_fact_key=matched_fact_key,
        )


        claim_verdict.additional_properties = d
        return claim_verdict

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
