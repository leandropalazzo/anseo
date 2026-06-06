from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.audit_finding_category import AuditFindingCategory
from ..models.audit_finding_severity import AuditFindingSeverity
from ..models.audit_finding_status import AuditFindingStatus
from typing import cast






T = TypeVar("T", bound="AuditFinding")



@_attrs_define
class AuditFinding:
    """ 
        Attributes:
            rule_id (str):
            category (AuditFindingCategory):
            severity (AuditFindingSeverity):
            status (AuditFindingStatus):
            score (int):
            message (str):
            recommendation_kind (str):
            evidence (list[str]):
     """

    rule_id: str
    category: AuditFindingCategory
    severity: AuditFindingSeverity
    status: AuditFindingStatus
    score: int
    message: str
    recommendation_kind: str
    evidence: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        rule_id = self.rule_id

        category = self.category.value

        severity = self.severity.value

        status = self.status.value

        score = self.score

        message = self.message

        recommendation_kind = self.recommendation_kind

        evidence = self.evidence




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "rule_id": rule_id,
            "category": category,
            "severity": severity,
            "status": status,
            "score": score,
            "message": message,
            "recommendation_kind": recommendation_kind,
            "evidence": evidence,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        rule_id = d.pop("rule_id")

        category = AuditFindingCategory(d.pop("category"))




        severity = AuditFindingSeverity(d.pop("severity"))




        status = AuditFindingStatus(d.pop("status"))




        score = d.pop("score")

        message = d.pop("message")

        recommendation_kind = d.pop("recommendation_kind")

        evidence = cast(list[str], d.pop("evidence"))


        audit_finding = cls(
            rule_id=rule_id,
            category=category,
            severity=severity,
            status=status,
            score=score,
            message=message,
            recommendation_kind=recommendation_kind,
            evidence=evidence,
        )


        audit_finding.additional_properties = d
        return audit_finding

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
