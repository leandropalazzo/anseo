from typing import TYPE_CHECKING, Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.gate_summary import GateSummary
    from ..models.page_audit import PageAudit


T = TypeVar("T", bound="AuditReport")


@_attrs_define
class AuditReport:
    """
    Attributes:
        target (str):
        overall_score (int):
        pages (list['PageAudit']):
        gate (Union['GateSummary', None, Unset]):
    """

    target: str
    overall_score: int
    pages: list["PageAudit"]
    gate: Union["GateSummary", None, Unset] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.gate_summary import GateSummary

        target = self.target

        overall_score = self.overall_score

        pages = []
        for pages_item_data in self.pages:
            pages_item = pages_item_data.to_dict()
            pages.append(pages_item)

        gate: Union[None, Unset, dict[str, Any]]
        if isinstance(self.gate, Unset):
            gate = UNSET
        elif isinstance(self.gate, GateSummary):
            gate = self.gate.to_dict()
        else:
            gate = self.gate

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "target": target,
                "overall_score": overall_score,
                "pages": pages,
            }
        )
        if gate is not UNSET:
            field_dict["gate"] = gate

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        from ..models.gate_summary import GateSummary
        from ..models.page_audit import PageAudit

        d = src_dict.copy()
        target = d.pop("target")

        overall_score = d.pop("overall_score")

        pages = []
        _pages = d.pop("pages")
        for pages_item_data in _pages:
            pages_item = PageAudit.from_dict(pages_item_data)

            pages.append(pages_item)

        def _parse_gate(data: object) -> Union["GateSummary", None, Unset]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                gate_type_1 = GateSummary.from_dict(data)

                return gate_type_1
            except:  # noqa: E722
                pass
            return cast(Union["GateSummary", None, Unset], data)

        gate = _parse_gate(d.pop("gate", UNSET))

        audit_report = cls(
            target=target,
            overall_score=overall_score,
            pages=pages,
            gate=gate,
        )

        audit_report.additional_properties = d
        return audit_report

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
