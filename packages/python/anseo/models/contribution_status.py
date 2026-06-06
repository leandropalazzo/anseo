from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.contribution_status_status import ContributionStatusStatus
from ..types import UNSET, Unset

T = TypeVar("T", bound="ContributionStatus")


@_attrs_define
class ContributionStatus:
    """Outcome of the benchmark consent + envelope gate. `sealed`: opted in with a KEK, redacted + sealed.
    `skipped_not_opted_in`: no active opt-in. `kek_missing`: opted in but no per-project KEK available, so the
    contribution could NOT be sealed (the run is still recorded; benchmark data is flagged, never silently dropped).
    `redaction_rejected`: redaction refused the run (e.g. stale consent terms).

        Attributes:
            status (ContributionStatusStatus):
            reason (Union[None, Unset, str]): Present only when status is redaction_rejected.
    """

    status: ContributionStatusStatus
    reason: Union[None, Unset, str] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        status = self.status.value

        reason: Union[None, Unset, str]
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "status": status,
            }
        )
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        status = ContributionStatusStatus(d.pop("status"))

        def _parse_reason(data: object) -> Union[None, Unset, str]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, str], data)

        reason = _parse_reason(d.pop("reason", UNSET))

        contribution_status = cls(
            status=status,
            reason=reason,
        )

        contribution_status.additional_properties = d
        return contribution_status

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
