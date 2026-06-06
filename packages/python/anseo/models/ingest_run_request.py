import datetime
from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field
from dateutil.parser import isoparse

from ..types import UNSET, Unset

T = TypeVar("T", bound="IngestRunRequest")


@_attrs_define
class IngestRunRequest:
    """One externally-executed prompt run submitted for ingestion.

    Attributes:
        prompt_slug (str): Slug-safe identifier of a prompt already declared in the resolved project.
        provider (str): Provider the external run was executed against (e.g. openai).
        model (str): Provider model/version string the external run used.
        response_text (Union[None, Unset, str]): Raw provider response text. Optional when citation_domains is supplied
            directly.
        citation_domains (Union[None, Unset, list[str]]): Source domains observed in citations. When omitted, extracted
            from response_text.
        observed_rank (Union[None, Unset, int]): The brand's observed rank in this run, if computed by the caller.
        observed_at (Union[None, Unset, datetime.datetime]): When the external run was observed. Defaults to now.
        contribute (Union[Unset, bool]): Opt this run into the anonymous benchmark. Defaults to false. A true value with
            no per-project KEK is rejected 403 kek_missing. When the project also has an active benchmark opt-in on the
            current terms, a true run is redacted and envelope-sealed under the project KEK (Story 40.4); otherwise the run
            is recorded but the contribution is reported as skipped/blocked, never silently dropped. Default: False.
    """

    prompt_slug: str
    provider: str
    model: str
    response_text: Union[None, Unset, str] = UNSET
    citation_domains: Union[None, Unset, list[str]] = UNSET
    observed_rank: Union[None, Unset, int] = UNSET
    observed_at: Union[None, Unset, datetime.datetime] = UNSET
    contribute: Union[Unset, bool] = False
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        prompt_slug = self.prompt_slug

        provider = self.provider

        model = self.model

        response_text: Union[None, Unset, str]
        if isinstance(self.response_text, Unset):
            response_text = UNSET
        else:
            response_text = self.response_text

        citation_domains: Union[None, Unset, list[str]]
        if isinstance(self.citation_domains, Unset):
            citation_domains = UNSET
        elif isinstance(self.citation_domains, list):
            citation_domains = self.citation_domains

        else:
            citation_domains = self.citation_domains

        observed_rank: Union[None, Unset, int]
        if isinstance(self.observed_rank, Unset):
            observed_rank = UNSET
        else:
            observed_rank = self.observed_rank

        observed_at: Union[None, Unset, str]
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
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        prompt_slug = d.pop("prompt_slug")

        provider = d.pop("provider")

        model = d.pop("model")

        def _parse_response_text(data: object) -> Union[None, Unset, str]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, str], data)

        response_text = _parse_response_text(d.pop("response_text", UNSET))

        def _parse_citation_domains(data: object) -> Union[None, Unset, list[str]]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                citation_domains_type_0 = cast(list[str], data)

                return citation_domains_type_0
            except:  # noqa: E722
                pass
            return cast(Union[None, Unset, list[str]], data)

        citation_domains = _parse_citation_domains(d.pop("citation_domains", UNSET))

        def _parse_observed_rank(data: object) -> Union[None, Unset, int]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, int], data)

        observed_rank = _parse_observed_rank(d.pop("observed_rank", UNSET))

        def _parse_observed_at(data: object) -> Union[None, Unset, datetime.datetime]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                observed_at_type_0 = isoparse(data)

                return observed_at_type_0
            except:  # noqa: E722
                pass
            return cast(Union[None, Unset, datetime.datetime], data)

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
