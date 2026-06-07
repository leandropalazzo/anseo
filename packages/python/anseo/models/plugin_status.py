from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.plugin_status_kind import PluginStatusKind
from ..models.plugin_status_status import PluginStatusStatus
from ..types import UNSET, Unset

T = TypeVar("T", bound="PluginStatus")


@_attrs_define
class PluginStatus:
    """Story 41.2 — runtime activation status of one installed plugin, as resolved at `anseo serve` boot. `anseo plugin
    list` renders the same fields.

        Attributes:
            id (str): Plugin id (`namespace/name`).
            kind (PluginStatusKind):
            status (PluginStatusStatus):
            version (str):
            reason (str | Unset): Human-readable reason for a `skipped` / `load_error` outcome; absent when `loaded`.
    """

    id: str
    kind: PluginStatusKind
    status: PluginStatusStatus
    version: str
    reason: str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        id = self.id

        kind = self.kind.value

        status = self.status.value

        version = self.version

        reason = self.reason

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "id": id,
                "kind": kind,
                "status": status,
                "version": version,
            }
        )
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        id = d.pop("id")

        kind = PluginStatusKind(d.pop("kind"))

        status = PluginStatusStatus(d.pop("status"))

        version = d.pop("version")

        reason = d.pop("reason", UNSET)

        plugin_status = cls(
            id=id,
            kind=kind,
            status=status,
            version=version,
            reason=reason,
        )

        plugin_status.additional_properties = d
        return plugin_status

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
