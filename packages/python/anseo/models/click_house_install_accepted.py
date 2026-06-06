from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ClickHouseInstallAccepted")


@_attrs_define
class ClickHouseInstallAccepted:
    """Story 15.1 — 202 response from POST /v1/setup/clickhouse/install. `install_id` is a ULID the caller can use to
    subscribe to the SSE progress stream. MOCK in 15.1; real Docker calls land in Story 15.3.

        Attributes:
            install_id (str): ULID identifying the install.
            stream (str): Path to the SSE progress stream.
    """

    install_id: str
    stream: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        install_id = self.install_id

        stream = self.stream

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "install_id": install_id,
                "stream": stream,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        install_id = d.pop("install_id")

        stream = d.pop("stream")

        click_house_install_accepted = cls(
            install_id=install_id,
            stream=stream,
        )

        click_house_install_accepted.additional_properties = d
        return click_house_install_accepted

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
