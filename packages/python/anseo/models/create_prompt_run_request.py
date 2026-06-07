from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.create_prompt_run_request_provider import CreatePromptRunRequestProvider
from ..types import UNSET, Unset

T = TypeVar("T", bound="CreatePromptRunRequest")


@_attrs_define
class CreatePromptRunRequest:
    """
    Attributes:
        prompt_name (str): Slug-safe prompt identifier declared in anseo.yaml.
        provider (CreatePromptRunRequestProvider):
        triggered_by (None | str | Unset):
    """

    prompt_name: str
    provider: CreatePromptRunRequestProvider
    triggered_by: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        prompt_name = self.prompt_name

        provider = self.provider.value

        triggered_by: None | str | Unset
        if isinstance(self.triggered_by, Unset):
            triggered_by = UNSET
        else:
            triggered_by = self.triggered_by

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "prompt_name": prompt_name,
                "provider": provider,
            }
        )
        if triggered_by is not UNSET:
            field_dict["triggered_by"] = triggered_by

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        prompt_name = d.pop("prompt_name")

        provider = CreatePromptRunRequestProvider(d.pop("provider"))

        def _parse_triggered_by(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        triggered_by = _parse_triggered_by(d.pop("triggered_by", UNSET))

        create_prompt_run_request = cls(
            prompt_name=prompt_name,
            provider=provider,
            triggered_by=triggered_by,
        )

        create_prompt_run_request.additional_properties = d
        return create_prompt_run_request

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
