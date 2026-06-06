import datetime
from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field
from dateutil.parser import isoparse

from ..types import UNSET, Unset

T = TypeVar("T", bound="SetupStatusWebhookTarget")


@_attrs_define
class SetupStatusWebhookTarget:
    """
    Attributes:
        configured (bool):
        error (Union[Unset, str]):
        last_delivery_at (Union[None, Unset, datetime.datetime]):
        last_status (Union[None, Unset, str]):
    """

    configured: bool
    error: Union[Unset, str] = UNSET
    last_delivery_at: Union[None, Unset, datetime.datetime] = UNSET
    last_status: Union[None, Unset, str] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        configured = self.configured

        error = self.error

        last_delivery_at: Union[None, Unset, str]
        if isinstance(self.last_delivery_at, Unset):
            last_delivery_at = UNSET
        elif isinstance(self.last_delivery_at, datetime.datetime):
            last_delivery_at = self.last_delivery_at.isoformat()
        else:
            last_delivery_at = self.last_delivery_at

        last_status: Union[None, Unset, str]
        if isinstance(self.last_status, Unset):
            last_status = UNSET
        else:
            last_status = self.last_status

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "configured": configured,
            }
        )
        if error is not UNSET:
            field_dict["error"] = error
        if last_delivery_at is not UNSET:
            field_dict["last_delivery_at"] = last_delivery_at
        if last_status is not UNSET:
            field_dict["last_status"] = last_status

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        configured = d.pop("configured")

        error = d.pop("error", UNSET)

        def _parse_last_delivery_at(
            data: object,
        ) -> Union[None, Unset, datetime.datetime]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                last_delivery_at_type_0 = isoparse(data)

                return last_delivery_at_type_0
            except:  # noqa: E722
                pass
            return cast(Union[None, Unset, datetime.datetime], data)

        last_delivery_at = _parse_last_delivery_at(d.pop("last_delivery_at", UNSET))

        def _parse_last_status(data: object) -> Union[None, Unset, str]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, str], data)

        last_status = _parse_last_status(d.pop("last_status", UNSET))

        setup_status_webhook_target = cls(
            configured=configured,
            error=error,
            last_delivery_at=last_delivery_at,
            last_status=last_status,
        )

        setup_status_webhook_target.additional_properties = d
        return setup_status_webhook_target

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
