from enum import Enum


class ClickHouseInstallEventStep(str, Enum):
    APPLYING_MIGRATIONS = "applying_migrations"
    COMPLETE = "complete"
    CONTAINER_STARTING = "container_starting"
    DOCKER_DETECTED = "docker_detected"
    IMAGE_PULLING = "image_pulling"
    PROVISIONING_USER = "provisioning_user"
    RUNNING_PARITY_TEST = "running_parity_test"

    def __str__(self) -> str:
        return str(self.value)
