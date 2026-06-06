import datetime
from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field
from dateutil.parser import isoparse

from ..models.setup_status_postgres_state import SetupStatusPostgresState
from ..types import UNSET, Unset

T = TypeVar("T", bound="SetupStatusPostgres")


@_attrs_define
class SetupStatusPostgres:
    """
    Attributes:
        state (SetupStatusPostgresState):
        error (Union[Unset, str]):
        last_write_at (Union[None, Unset, datetime.datetime]):
        row_count_estimate (Union[None, Unset, int]):
        schema_version (Union[None, Unset, int]):
    """

    state: SetupStatusPostgresState
    error: Union[Unset, str] = UNSET
    last_write_at: Union[None, Unset, datetime.datetime] = UNSET
    row_count_estimate: Union[None, Unset, int] = UNSET
    schema_version: Union[None, Unset, int] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        state = self.state.value

        error = self.error

        last_write_at: Union[None, Unset, str]
        if isinstance(self.last_write_at, Unset):
            last_write_at = UNSET
        elif isinstance(self.last_write_at, datetime.datetime):
            last_write_at = self.last_write_at.isoformat()
        else:
            last_write_at = self.last_write_at

        row_count_estimate: Union[None, Unset, int]
        if isinstance(self.row_count_estimate, Unset):
            row_count_estimate = UNSET
        else:
            row_count_estimate = self.row_count_estimate

        schema_version: Union[None, Unset, int]
        if isinstance(self.schema_version, Unset):
            schema_version = UNSET
        else:
            schema_version = self.schema_version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "state": state,
            }
        )
        if error is not UNSET:
            field_dict["error"] = error
        if last_write_at is not UNSET:
            field_dict["last_write_at"] = last_write_at
        if row_count_estimate is not UNSET:
            field_dict["row_count_estimate"] = row_count_estimate
        if schema_version is not UNSET:
            field_dict["schema_version"] = schema_version

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        state = SetupStatusPostgresState(d.pop("state"))

        error = d.pop("error", UNSET)

        def _parse_last_write_at(data: object) -> Union[None, Unset, datetime.datetime]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                last_write_at_type_0 = isoparse(data)

                return last_write_at_type_0
            except:  # noqa: E722
                pass
            return cast(Union[None, Unset, datetime.datetime], data)

        last_write_at = _parse_last_write_at(d.pop("last_write_at", UNSET))

        def _parse_row_count_estimate(data: object) -> Union[None, Unset, int]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, int], data)

        row_count_estimate = _parse_row_count_estimate(
            d.pop("row_count_estimate", UNSET)
        )

        def _parse_schema_version(data: object) -> Union[None, Unset, int]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, int], data)

        schema_version = _parse_schema_version(d.pop("schema_version", UNSET))

        setup_status_postgres = cls(
            state=state,
            error=error,
            last_write_at=last_write_at,
            row_count_estimate=row_count_estimate,
            schema_version=schema_version,
        )

        setup_status_postgres.additional_properties = d
        return setup_status_postgres

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
