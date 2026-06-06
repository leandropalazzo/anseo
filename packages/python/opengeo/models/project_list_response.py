from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.project_view import ProjectView





T = TypeVar("T", bound="ProjectListResponse")



@_attrs_define
class ProjectListResponse:
    """ 
        Attributes:
            projects (list[ProjectView]):
     """

    projects: list[ProjectView]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.project_view import ProjectView
        projects = []
        for projects_item_data in self.projects:
            projects_item = projects_item_data.to_dict()
            projects.append(projects_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "projects": projects,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.project_view import ProjectView
        d = dict(src_dict)
        projects = []
        _projects = d.pop("projects")
        for projects_item_data in (_projects):
            projects_item = ProjectView.from_dict(projects_item_data)



            projects.append(projects_item)


        project_list_response = cls(
            projects=projects,
        )


        project_list_response.additional_properties = d
        return project_list_response

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
