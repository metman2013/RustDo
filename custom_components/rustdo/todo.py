"""Each RustDo list (yours, and family-shared ones you're in) as a to-do list."""

from __future__ import annotations

from datetime import date, datetime

from homeassistant.components.todo import (
    TodoItem,
    TodoItemStatus,
    TodoListEntity,
    TodoListEntityFeature,
)
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError
from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from . import RustDoConfigEntry, RustDoCoordinator
from .api import HubError, ListView
from .const import DOMAIN


async def async_setup_entry(hass: HomeAssistant, entry: RustDoConfigEntry, async_add_entities: AddEntitiesCallback) -> None:
    coordinator = entry.runtime_data
    known: set[str] = set()

    @callback
    def add_new_lists() -> None:
        new = [RustDoList(coordinator, lv.id) for lv in coordinator.data or [] if lv.id not in known]
        known.update(e.list_id for e in new)
        if new:
            async_add_entities(new)

    add_new_lists()
    # Lists made in RustDo later show up too.
    entry.async_on_unload(coordinator.async_add_listener(add_new_lists))


def _due(d: date | datetime | None) -> date | None:
    if isinstance(d, datetime):
        return d.date()
    return d


class RustDoList(CoordinatorEntity[RustDoCoordinator], TodoListEntity):
    _attr_has_entity_name = True
    _attr_supported_features = (
        TodoListEntityFeature.CREATE_TODO_ITEM
        | TodoListEntityFeature.UPDATE_TODO_ITEM
        | TodoListEntityFeature.DELETE_TODO_ITEM
        | TodoListEntityFeature.SET_DUE_DATE_ON_ITEM
        | TodoListEntityFeature.SET_DESCRIPTION_ON_ITEM
    )

    def __init__(self, coordinator: RustDoCoordinator, list_id: str) -> None:
        super().__init__(coordinator)
        self.list_id = list_id
        entry = coordinator.config_entry
        self._attr_unique_id = f"{entry.unique_id}_{list_id}"
        self._attr_device_info = DeviceInfo(
            identifiers={(DOMAIN, entry.unique_id or entry.entry_id)},
            name="RustDo",
            manufacturer="RustDo",
            configuration_url="https://rustdo.app",
        )

    @property
    def _list(self) -> ListView | None:
        return next((lv for lv in self.coordinator.data or [] if lv.id == self.list_id), None)

    @property
    def available(self) -> bool:
        return super().available and self._list is not None

    @property
    def name(self) -> str | None:
        lv = self._list
        return lv.name if lv else None

    @property
    def icon(self) -> str:
        lv = self._list
        return "mdi:account-group" if lv and lv.code else "mdi:format-list-checks"

    @property
    def todo_items(self) -> list[TodoItem] | None:
        lv = self._list
        if lv is None:
            return None
        return [
            TodoItem(
                summary=t.title,
                uid=t.id,
                status=TodoItemStatus.COMPLETED if t.done else TodoItemStatus.NEEDS_ACTION,
                due=t.due,
                description=t.notes or None,
            )
            for t in lv.tasks
        ]

    async def async_create_todo_item(self, item: TodoItem) -> None:
        try:
            task_id = await self.coordinator.client.add_task(
                self.list_id, item.summary or "", _due(item.due), item.description or ""
            )
            if item.status == TodoItemStatus.COMPLETED:
                await self.coordinator.client.update_task(self.list_id, task_id, done=True)
        except HubError as e:
            raise HomeAssistantError(f"RustDo: {e}") from e
        self.coordinator.changed()

    async def async_update_todo_item(self, item: TodoItem) -> None:
        if item.uid is None:
            return
        try:
            await self.coordinator.client.update_task(
                self.list_id,
                item.uid,
                title=item.summary,
                done=None if item.status is None else item.status == TodoItemStatus.COMPLETED,
                due=_due(item.due),
                notes=item.description or "",
            )
        except HubError as e:
            raise HomeAssistantError(f"RustDo: {e}") from e
        self.coordinator.changed()

    async def async_delete_todo_items(self, uids: list[str]) -> None:
        try:
            await self.coordinator.client.delete_tasks(self.list_id, uids)
        except HubError as e:
            raise HomeAssistantError(f"RustDo: {e}") from e
        self.coordinator.changed()
