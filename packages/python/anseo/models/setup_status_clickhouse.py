from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.setup_status_clickhouse_state import SetupStatusClickhouseState
from ..types import UNSET, Unset

T = TypeVar("T", bound="SetupStatusClickhouse")


@_attrs_define
class SetupStatusClickhouse:
    """
    Attributes:
        state (SetupStatusClickhouseState):
        error (str | Unset):
        etl_lag_seconds (float | None | Unset):
        row_count (int | None | Unset):
        url (None | str | Unset):
    """

    state: SetupStatusClickhouseState
    error: str | Unset = UNSET
    etl_lag_seconds: float | None | Unset = UNSET
    row_count: int | None | Unset = UNSET
    url: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        state = self.state.value

        error = self.error

        etl_lag_seconds: float | None | Unset
        if isinstance(self.etl_lag_seconds, Unset):
            etl_lag_seconds = UNSET
        else:
            etl_lag_seconds = self.etl_lag_seconds

        row_count: int | None | Unset
        if isinstance(self.row_count, Unset):
            row_count = UNSET
        else:
            row_count = self.row_count

        url: None | str | Unset
        if isinstance(self.url, Unset):
            url = UNSET
        else:
            url = self.url

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "state": state,
            }
        )
        if error is not UNSET:
            field_dict["error"] = error
        if etl_lag_seconds is not UNSET:
            field_dict["etl_lag_seconds"] = etl_lag_seconds
        if row_count is not UNSET:
            field_dict["row_count"] = row_count
        if url is not UNSET:
            field_dict["url"] = url

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        state = SetupStatusClickhouseState(d.pop("state"))

        error = d.pop("error", UNSET)

        def _parse_etl_lag_seconds(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        etl_lag_seconds = _parse_etl_lag_seconds(d.pop("etl_lag_seconds", UNSET))

        def _parse_row_count(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        row_count = _parse_row_count(d.pop("row_count", UNSET))

        def _parse_url(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        url = _parse_url(d.pop("url", UNSET))

        setup_status_clickhouse = cls(
            state=state,
            error=error,
            etl_lag_seconds=etl_lag_seconds,
            row_count=row_count,
            url=url,
        )

        setup_status_clickhouse.additional_properties = d
        return setup_status_clickhouse

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
