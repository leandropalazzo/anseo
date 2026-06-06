from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.setup_status_worker_state import SetupStatusWorkerState
from ..types import UNSET, Unset

T = TypeVar("T", bound="SetupStatusWorker")


@_attrs_define
class SetupStatusWorker:
    """
    Attributes:
        state (SetupStatusWorkerState):
        error (Union[Unset, str]):
        queue_depth (Union[None, Unset, int]):
        uptime_seconds (Union[None, Unset, int]):
    """

    state: SetupStatusWorkerState
    error: Union[Unset, str] = UNSET
    queue_depth: Union[None, Unset, int] = UNSET
    uptime_seconds: Union[None, Unset, int] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        state = self.state.value

        error = self.error

        queue_depth: Union[None, Unset, int]
        if isinstance(self.queue_depth, Unset):
            queue_depth = UNSET
        else:
            queue_depth = self.queue_depth

        uptime_seconds: Union[None, Unset, int]
        if isinstance(self.uptime_seconds, Unset):
            uptime_seconds = UNSET
        else:
            uptime_seconds = self.uptime_seconds

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "state": state,
            }
        )
        if error is not UNSET:
            field_dict["error"] = error
        if queue_depth is not UNSET:
            field_dict["queue_depth"] = queue_depth
        if uptime_seconds is not UNSET:
            field_dict["uptime_seconds"] = uptime_seconds

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        state = SetupStatusWorkerState(d.pop("state"))

        error = d.pop("error", UNSET)

        def _parse_queue_depth(data: object) -> Union[None, Unset, int]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, int], data)

        queue_depth = _parse_queue_depth(d.pop("queue_depth", UNSET))

        def _parse_uptime_seconds(data: object) -> Union[None, Unset, int]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, int], data)

        uptime_seconds = _parse_uptime_seconds(d.pop("uptime_seconds", UNSET))

        setup_status_worker = cls(
            state=state,
            error=error,
            queue_depth=queue_depth,
            uptime_seconds=uptime_seconds,
        )

        setup_status_worker.additional_properties = d
        return setup_status_worker

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
