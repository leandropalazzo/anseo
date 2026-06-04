from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.click_house_install_event_step import ClickHouseInstallEventStep
from dateutil.parser import isoparse
from typing import cast
import datetime






T = TypeVar("T", bound="ClickHouseInstallEvent")



@_attrs_define
class ClickHouseInstallEvent:
    """ Story 15.1 — one frame of the SSE install stream. Step ordering: docker_detected → image_pulling →
    container_starting → provisioning_user → applying_migrations → running_parity_test → complete.

        Attributes:
            at (datetime.datetime):
            log_line (str):
            progress (float):
            step (ClickHouseInstallEventStep):
     """

    at: datetime.datetime
    log_line: str
    progress: float
    step: ClickHouseInstallEventStep
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        at = self.at.isoformat()

        log_line = self.log_line

        progress = self.progress

        step = self.step.value


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "at": at,
            "log_line": log_line,
            "progress": progress,
            "step": step,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        at = isoparse(d.pop("at"))




        log_line = d.pop("log_line")

        progress = d.pop("progress")

        step = ClickHouseInstallEventStep(d.pop("step"))




        click_house_install_event = cls(
            at=at,
            log_line=log_line,
            progress=progress,
            step=step,
        )


        click_house_install_event.additional_properties = d
        return click_house_install_event

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
