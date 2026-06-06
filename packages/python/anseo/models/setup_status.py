from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.setup_status_api_keys_item import SetupStatusApiKeysItem
    from ..models.setup_status_clickhouse import SetupStatusClickhouse
    from ..models.setup_status_docker import SetupStatusDocker
    from ..models.setup_status_postgres import SetupStatusPostgres
    from ..models.setup_status_webhook_target import SetupStatusWebhookTarget
    from ..models.setup_status_worker import SetupStatusWorker


T = TypeVar("T", bound="SetupStatus")


@_attrs_define
class SetupStatus:
    """Story 15.1 — best-effort status probe across all deployment surfaces. Always returned 200; individual sections carry
    `state: "unknown"` + an `error` string on failure (per-probe timeout: 1s; 500ms for Docker).

        Attributes:
            api_keys (list[SetupStatusApiKeysItem]):
            clickhouse (SetupStatusClickhouse):
            docker (SetupStatusDocker):
            postgres (SetupStatusPostgres):
            webhook_target (SetupStatusWebhookTarget):
            worker (SetupStatusWorker):
    """

    api_keys: list[SetupStatusApiKeysItem]
    clickhouse: SetupStatusClickhouse
    docker: SetupStatusDocker
    postgres: SetupStatusPostgres
    webhook_target: SetupStatusWebhookTarget
    worker: SetupStatusWorker
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        api_keys = []
        for api_keys_item_data in self.api_keys:
            api_keys_item = api_keys_item_data.to_dict()
            api_keys.append(api_keys_item)

        clickhouse = self.clickhouse.to_dict()

        docker = self.docker.to_dict()

        postgres = self.postgres.to_dict()

        webhook_target = self.webhook_target.to_dict()

        worker = self.worker.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "api_keys": api_keys,
                "clickhouse": clickhouse,
                "docker": docker,
                "postgres": postgres,
                "webhook_target": webhook_target,
                "worker": worker,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.setup_status_api_keys_item import SetupStatusApiKeysItem
        from ..models.setup_status_clickhouse import SetupStatusClickhouse
        from ..models.setup_status_docker import SetupStatusDocker
        from ..models.setup_status_postgres import SetupStatusPostgres
        from ..models.setup_status_webhook_target import SetupStatusWebhookTarget
        from ..models.setup_status_worker import SetupStatusWorker

        d = dict(src_dict)
        api_keys = []
        _api_keys = d.pop("api_keys")
        for api_keys_item_data in _api_keys:
            api_keys_item = SetupStatusApiKeysItem.from_dict(api_keys_item_data)

            api_keys.append(api_keys_item)

        clickhouse = SetupStatusClickhouse.from_dict(d.pop("clickhouse"))

        docker = SetupStatusDocker.from_dict(d.pop("docker"))

        postgres = SetupStatusPostgres.from_dict(d.pop("postgres"))

        webhook_target = SetupStatusWebhookTarget.from_dict(d.pop("webhook_target"))

        worker = SetupStatusWorker.from_dict(d.pop("worker"))

        setup_status = cls(
            api_keys=api_keys,
            clickhouse=clickhouse,
            docker=docker,
            postgres=postgres,
            webhook_target=webhook_target,
            worker=worker,
        )

        setup_status.additional_properties = d
        return setup_status

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
