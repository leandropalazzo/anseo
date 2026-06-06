from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.transition_recommendation_request_to import (
    TransitionRecommendationRequestTo,
)
from ..types import UNSET, Unset

T = TypeVar("T", bound="TransitionRecommendationRequest")


@_attrs_define
class TransitionRecommendationRequest:
    """Story 19.6 — lifecycle transition (Story 19.4 state machine). Illegal edges return 409.

    Attributes:
        to (TransitionRecommendationRequestTo):
        evidence_url (Union[None, Unset, str]):
        note (Union[None, Unset, str]):
    """

    to: TransitionRecommendationRequestTo
    evidence_url: Union[None, Unset, str] = UNSET
    note: Union[None, Unset, str] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        to = self.to.value

        evidence_url: Union[None, Unset, str]
        if isinstance(self.evidence_url, Unset):
            evidence_url = UNSET
        else:
            evidence_url = self.evidence_url

        note: Union[None, Unset, str]
        if isinstance(self.note, Unset):
            note = UNSET
        else:
            note = self.note

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "to": to,
            }
        )
        if evidence_url is not UNSET:
            field_dict["evidence_url"] = evidence_url
        if note is not UNSET:
            field_dict["note"] = note

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        to = TransitionRecommendationRequestTo(d.pop("to"))

        def _parse_evidence_url(data: object) -> Union[None, Unset, str]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, str], data)

        evidence_url = _parse_evidence_url(d.pop("evidence_url", UNSET))

        def _parse_note(data: object) -> Union[None, Unset, str]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, str], data)

        note = _parse_note(d.pop("note", UNSET))

        transition_recommendation_request = cls(
            to=to,
            evidence_url=evidence_url,
            note=note,
        )

        transition_recommendation_request.additional_properties = d
        return transition_recommendation_request

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
