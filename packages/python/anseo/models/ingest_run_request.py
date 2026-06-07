from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="IngestRunRequest")


@_attrs_define
class IngestRunRequest:
    """One externally-executed prompt run submitted for ingestion.

    Attributes:
        prompt_slug (str): Slug-safe identifier of a prompt already declared in the resolved project.
        provider (str): Provider the external run was executed against (e.g. openai).
        model (str): Provider model/version string the external run used.
        response_text (None | str | Unset): Raw provider response text. Optional when citation_domains is supplied
            directly.
        citation_domains (list[str] | None | Unset): Source domains observed in citations. When omitted, extracted from
            response_text.
        observed_rank (int | None | Unset): The brand's observed rank in this run, if computed by the caller.
        observed_at (datetime.datetime | None | Unset): When the external run was observed. Defaults to now.
        contribute (bool | Unset): Opt this run into the anonymous benchmark. Defaults to false. A true value with no
            per-project KEK is rejected 403 kek_missing. When the project also has an active benchmark opt-in on the current
            terms, a true run is redacted and envelope-sealed under the project KEK (Story 40.4); otherwise the run is
            recorded but the contribution is reported as skipped/blocked, never silently dropped. Default: False.
    """

    prompt_slug: str
    provider: str
    model: str
    response_text: None | str | Unset = UNSET
    citation_domains: list[str] | None | Unset = UNSET
    observed_rank: int | None | Unset = UNSET
    observed_at: datetime.datetime | None | Unset = UNSET
    contribute: bool | Unset = False
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        prompt_slug = self.prompt_slug

        provider = self.provider

        model = self.model

        response_text: None | str | Unset
        if isinstance(self.response_text, Unset):
            response_text = UNSET
        else:
            response_text = self.response_text

        citation_domains: list[str] | None | Unset
        if isinstance(self.citation_domains, Unset):
            citation_domains = UNSET
        elif isinstance(self.citation_domains, list):
            citation_domains = self.citation_domains

        else:
            citation_domains = self.citation_domains

        observed_rank: int | None | Unset
        if isinstance(self.observed_rank, Unset):
            observed_rank = UNSET
        else:
            observed_rank = self.observed_rank

        observed_at: None | str | Unset
        if isinstance(self.observed_at, Unset):
            observed_at = UNSET
        elif isinstance(self.observed_at, datetime.datetime):
            observed_at = self.observed_at.isoformat()
        else:
            observed_at = self.observed_at

        contribute = self.contribute

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "prompt_slug": prompt_slug,
                "provider": provider,
                "model": model,
            }
        )
        if response_text is not UNSET:
            field_dict["response_text"] = response_text
        if citation_domains is not UNSET:
            field_dict["citation_domains"] = citation_domains
        if observed_rank is not UNSET:
            field_dict["observed_rank"] = observed_rank
        if observed_at is not UNSET:
            field_dict["observed_at"] = observed_at
        if contribute is not UNSET:
            field_dict["contribute"] = contribute

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        prompt_slug = d.pop("prompt_slug")

        provider = d.pop("provider")

        model = d.pop("model")

        def _parse_response_text(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        response_text = _parse_response_text(d.pop("response_text", UNSET))

        def _parse_citation_domains(data: object) -> list[str] | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                citation_domains_type_0 = cast(list[str], data)

                return citation_domains_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(list[str] | None | Unset, data)

        citation_domains = _parse_citation_domains(d.pop("citation_domains", UNSET))

        def _parse_observed_rank(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        observed_rank = _parse_observed_rank(d.pop("observed_rank", UNSET))

        def _parse_observed_at(data: object) -> datetime.datetime | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                observed_at_type_0 = datetime.datetime.fromisoformat(data)

                return observed_at_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(datetime.datetime | None | Unset, data)

        observed_at = _parse_observed_at(d.pop("observed_at", UNSET))

        contribute = d.pop("contribute", UNSET)

        ingest_run_request = cls(
            prompt_slug=prompt_slug,
            provider=provider,
            model=model,
            response_text=response_text,
            citation_domains=citation_domains,
            observed_rank=observed_rank,
            observed_at=observed_at,
            contribute=contribute,
        )

        ingest_run_request.additional_properties = d
        return ingest_run_request

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
