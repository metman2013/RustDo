"""Set up RustDo: the hub's address and a sync key from RustDo's Settings."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

import voluptuous as vol

from homeassistant.config_entries import ConfigFlow, ConfigFlowResult
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .api import BadKey, HubError, RustDo, normalize_hub, normalize_key
from .const import CONF_HUB, CONF_KEY, DOMAIN


class RustDoConfigFlow(ConfigFlow, domain=DOMAIN):
    VERSION = 1

    async def _check(self, user_input: dict[str, Any]) -> tuple[dict[str, str], dict[str, str]]:
        """Returns (cleaned input, errors)."""
        errors: dict[str, str] = {}
        hub = normalize_hub(user_input.get(CONF_HUB, ""))
        key = normalize_key(user_input.get(CONF_KEY, ""))
        if hub is None:
            errors[CONF_HUB] = "bad_hub"
        if key is None:
            errors[CONF_KEY] = "bad_key"
        if errors:
            return {}, errors
        client = RustDo(async_get_clientsession(self.hass), hub, key)
        try:
            await client.refresh()
        except BadKey:
            errors[CONF_KEY] = "unknown_key"
        except HubError:
            errors["base"] = "cannot_connect"
        return {CONF_HUB: hub, CONF_KEY: key}, errors

    async def async_step_user(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        errors: dict[str, str] = {}
        if user_input is not None:
            data, errors = await self._check(user_input)
            if not errors:
                await self.async_set_unique_id(data[CONF_KEY])
                self._abort_if_unique_id_configured()
                return self.async_create_entry(title="RustDo", data=data)
        schema = vol.Schema(
            {
                vol.Required(CONF_HUB, default=(user_input or {}).get(CONF_HUB, "")): str,
                vol.Required(CONF_KEY, default=(user_input or {}).get(CONF_KEY, "")): str,
            }
        )
        return self.async_show_form(step_id="user", data_schema=schema, errors=errors)

    async def async_step_reauth(self, entry_data: Mapping[str, Any]) -> ConfigFlowResult:
        return await self.async_step_reauth_confirm()

    async def async_step_reauth_confirm(self, user_input: dict[str, Any] | None = None) -> ConfigFlowResult:
        entry = self._get_reauth_entry()
        errors: dict[str, str] = {}
        if user_input is not None:
            data, errors = await self._check({CONF_HUB: user_input.get(CONF_HUB, entry.data[CONF_HUB]), CONF_KEY: user_input[CONF_KEY]})
            if not errors:
                return self.async_update_reload_and_abort(entry, data=data, unique_id=data[CONF_KEY])
        schema = vol.Schema(
            {
                vol.Required(CONF_HUB, default=entry.data[CONF_HUB]): str,
                vol.Required(CONF_KEY): str,
            }
        )
        return self.async_show_form(step_id="reauth_confirm", data_schema=schema, errors=errors)
