from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="GenerateRecommendationsAccepted")


@_attrs_define
class GenerateRecommendationsAccepted:
    """Story 19.6 — 202 response from POST /v1/recommendations/generate. Per the Phase 2 async-write pattern, `status_url`
    points at the list endpoint where the results are readable.

        Attributes:
            generated_count (int):
            inserted_count (int):
            status (str):
            status_url (str):
    """

    generated_count: int
    inserted_count: int
    status: str
    status_url: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        generated_count = self.generated_count

        inserted_count = self.inserted_count

        status = self.status

        status_url = self.status_url

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "generated_count": generated_count,
                "inserted_count": inserted_count,
                "status": status,
                "status_url": status_url,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        generated_count = d.pop("generated_count")

        inserted_count = d.pop("inserted_count")

        status = d.pop("status")

        status_url = d.pop("status_url")

        generate_recommendations_accepted = cls(
            generated_count=generated_count,
            inserted_count=inserted_count,
            status=status,
            status_url=status_url,
        )

        generate_recommendations_accepted.additional_properties = d
        return generate_recommendations_accepted

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
