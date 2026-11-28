"""RustDo: your RustDo lists as Home Assistant to-do lists, through your RustDo Hub."""

from __future__ import annotations

import logging
from datetime import timedelta

from homeassistant.config_entries import ConfigEntry
from homeassistant.const import Platform
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.aiohttp_client import async_get_clientsession
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .api import BadKey, HubError, ListView, RustDo
from .const import CONF_HUB, CONF_KEY, DOMAIN, SCAN_SECONDS

_LOGGER = logging.getLogger(__name__)
PLATFORMS = [Platform.TODO]

type RustDoConfigEntry = ConfigEntry[RustDoCoordinator]


class RustDoCoordinator(DataUpdateCoordinator[list[ListView]]):
    """Keeps the lists current: every 30 seconds, and right after each change."""

    def __init__(self, hass: HomeAssistant, entry: ConfigEntry, client: RustDo) -> None:
        super().__init__(
            hass,
            _LOGGER,
            config_entry=entry,
            name=DOMAIN,
            update_interval=timedelta(seconds=SCAN_SECONDS),
        )
        self.client = client

    async def _async_update_data(self) -> list[ListView]:
        try:
            await self.client.refresh()
        except BadKey as e:
            raise ConfigEntryAuthFailed(str(e)) from e
        except HubError as e:
            raise UpdateFailed(str(e)) from e
        return self.client.lists()

    def changed(self) -> None:
        """After a change was sent: show what the hub now has."""
        self.async_set_updated_data(self.client.lists())


async def async_setup_entry(hass: HomeAssistant, entry: RustDoConfigEntry) -> bool:
    client = RustDo(async_get_clientsession(hass), entry.data[CONF_HUB], entry.data[CONF_KEY])
    coordinator = RustDoCoordinator(hass, entry, client)
    await coordinator.async_config_entry_first_refresh()
    entry.runtime_data = coordinator
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    return True


async def async_unload_entry(hass: HomeAssistant, entry: RustDoConfigEntry) -> bool:
    return await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
