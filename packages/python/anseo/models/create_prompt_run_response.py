from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="CreatePromptRunResponse")


@_attrs_define
class CreatePromptRunResponse:
    """
    Attributes:
        dispatched_at (datetime.datetime):
        project_id (str):
        prompt_name (str):
        provider (str):
        run_id (str):
        status (str):
    """

    dispatched_at: datetime.datetime
    project_id: str
    prompt_name: str
    provider: str
    run_id: str
    status: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        dispatched_at = self.dispatched_at.isoformat()

        project_id = self.project_id

        prompt_name = self.prompt_name

        provider = self.provider

        run_id = self.run_id

        status = self.status

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "dispatched_at": dispatched_at,
                "project_id": project_id,
                "prompt_name": prompt_name,
                "provider": provider,
                "run_id": run_id,
                "status": status,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        dispatched_at = datetime.datetime.fromisoformat(d.pop("dispatched_at"))

        project_id = d.pop("project_id")

        prompt_name = d.pop("prompt_name")

        provider = d.pop("provider")

        run_id = d.pop("run_id")

        status = d.pop("status")

        create_prompt_run_response = cls(
            dispatched_at=dispatched_at,
            project_id=project_id,
            prompt_name=prompt_name,
            provider=provider,
            run_id=run_id,
            status=status,
        )

        create_prompt_run_response.additional_properties = d
        return create_prompt_run_response

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
