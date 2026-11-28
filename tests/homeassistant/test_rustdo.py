"""The RustDo integration against a real hub: lists appear, and changes reach the hub the
way RustDo's own devices send them."""

from datetime import date

import aiohttp
from pytest_homeassistant_custom_component.common import MockConfigEntry

from homeassistant.components.todo import DOMAIN as TODO
from homeassistant.core import HomeAssistant

from custom_components.rustdo.api import RustDo, next_date, normalize_hub, normalize_key
from custom_components.rustdo.const import DOMAIN

LIST = "7c1f2b8e-0000-4000-8000-000000000001"
MILK = "7c1f2b8e-0000-4000-8000-0000000000a1"
BINS = "7c1f2b8e-0000-4000-8000-0000000000a2"


def task(title, **more):
    d = {"list_id": LIST, "title": title, "notes": "", "done": False, "important": False, "my_day": None,
         "due": None, "reminder": None, "recurrence": "Never", "steps": [], "created": "2026-11-28T09:00:00.000",
         "completed_at": None}
    d.update(more)
    return d


async def new_account(hub_url):
    """A fresh account with one list, as a RustDo device would have sent it."""
    async with aiohttp.ClientSession() as s:
        async with s.post(f"{hub_url}/api/account/new", json={}) as r:
            key = (await r.json())["key"]
        items = [
            {"kind": "list", "id": LIST, "modified": 1, "data": {"name": "Groceries", "pos": 1}},
            {"kind": "task", "id": MILK, "modified": 1, "data": task("Milk")},
            {"kind": "task", "id": BINS, "modified": 1,
             "data": task("Take out the bins", due="2026-11-28", recurrence="Weekly", repeat_every=2)},
        ]
        async with s.post(f"{hub_url}/api/account/sync", json={"key": key, "since": 0, "changes": items}) as r:
            assert r.status == 200
    return key


async def hub_tasks(hub_url, key):
    async with aiohttp.ClientSession() as s:
        c = RustDo(s, hub_url, key)
        await c.refresh()
        return {t.title: t for lv in c.lists() for t in lv.tasks}


async def setup(hass, hub_url, key):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=key, data={"hub": hub_url, "key": key})
    entry.add_to_hass(hass)
    assert await hass.config_entries.async_setup(entry.entry_id)
    await hass.async_block_till_done()
    return entry


def test_input_and_repeats():
    assert normalize_hub("192.168.1.20") == "http://192.168.1.20:7979"
    assert normalize_hub("https://todo.example.com/") == "https://todo.example.com"
    assert normalize_key("abcde fghjk mnpqr stuvw") == "ABCDE-FGHJK-MNPQR-STUVW"
    assert normalize_key("short") is None
    assert next_date("Weekly", date(2026, 11, 28), 2) == date(2026, 12, 12)
    assert next_date("Monthly", date(2026, 1, 31)) == date(2026, 2, 28)
    assert next_date("Weekdays", date(2026, 11, 27)) == date(2026, 11, 30)


async def test_lists_and_changes(hass: HomeAssistant, hub_url):
    key = await new_account(hub_url)
    await setup(hass, hub_url, key)

    state = hass.states.get("todo.rustdo_groceries")
    assert state is not None, hass.states.async_entity_ids(TODO)
    assert state.state == "2"  # open tasks

    items = await hass.services.async_call(TODO, "get_items", {}, target={"entity_id": "todo.rustdo_groceries"},
                                           blocking=True, return_response=True)
    titles = [i["summary"] for i in items["todo.rustdo_groceries"]["items"]]
    assert set(titles) == {"Milk", "Take out the bins"}

    # Add, with a due date: it reaches the hub as a RustDo task.
    await hass.services.async_call(TODO, "add_item", {"item": "Eggs", "due_date": "2026-12-01"},
                                   target={"entity_id": "todo.rustdo_groceries"}, blocking=True)
    t = await hub_tasks(hub_url, key)
    assert t["Eggs"].due == date(2026, 12, 1)
    assert t["Eggs"].data["list_id"] == LIST and t["Eggs"].data["recurrence"] == "Never"

    # Tick off a task that repeats every 2 weeks: the next one is made, 2 weeks on.
    await hass.services.async_call(TODO, "update_item", {"item": "Take out the bins", "status": "completed"},
                                   target={"entity_id": "todo.rustdo_groceries"}, blocking=True)
    async with aiohttp.ClientSession() as s:
        c = RustDo(s, hub_url, key)
        await c.refresh()
        bins = [t for t in c.lists()[0].tasks if t.title == "Take out the bins"]
    assert sorted((b.done, b.due) for b in bins) == [(False, date(2026, 12, 12)), (True, date(2026, 11, 28))]
    done = next(b for b in bins if b.done)
    assert done.data["recurrence"] == "Never" and done.data["completed_at"]

    # Rename, describe, remove.
    await hass.services.async_call(TODO, "update_item", {"item": "Milk", "rename": "Oat milk", "description": "2 cartons"},
                                   target={"entity_id": "todo.rustdo_groceries"}, blocking=True)
    t = await hub_tasks(hub_url, key)
    assert t["Oat milk"].notes == "2 cartons"
    await hass.services.async_call(TODO, "remove_item", {"item": "Oat milk"},
                                   target={"entity_id": "todo.rustdo_groceries"}, blocking=True)
    assert "Oat milk" not in await hub_tasks(hub_url, key)
    state = hass.states.get("todo.rustdo_groceries")
    assert state.state == "2"  # Eggs and the next bins


async def test_shared_list_shows_up(hass: HomeAssistant, hub_url):
    key = await new_account(hub_url)
    shared = "7c1f2b8e-0000-4000-8000-00000000beef"
    async with aiohttp.ClientSession() as s:
        async with s.post(f"{hub_url}/api/share", json={"list_id": shared}) as r:
            code = (await r.json())["code"]
        items = [{"kind": "list", "id": shared, "modified": 1, "data": {"name": "Family"}},
                 {"kind": "task", "id": "7c1f2b8e-0000-4000-8000-0000000000b1", "modified": 1,
                  "data": {k: v for k, v in task("Pick up Sam").items() if k not in ("list_id", "my_day")}}]
        await s.post(f"{hub_url}/api/sync", json={"code": code, "since": 0, "changes": items})
        share = {"kind": "share", "id": shared, "modified": 2, "data": {"hub": hub_url, "code": code, "name": "Family"}}
        await s.post(f"{hub_url}/api/account/sync", json={"key": key, "since": 0, "changes": [share]})
    await setup(hass, hub_url, key)
    assert hass.states.get("todo.rustdo_family").state == "1"
    await hass.services.async_call(TODO, "add_item", {"item": "Bake a cake"},
                                   target={"entity_id": "todo.rustdo_family"}, blocking=True)
    async with aiohttp.ClientSession() as s:
        async with s.post(f"{hub_url}/api/sync", json={"code": code, "since": 0, "changes": []}) as r:
            got = await r.json()
    titles = {i["data"]["title"] for i in got["changes"] if i["kind"] == "task"}
    assert titles == {"Pick up Sam", "Bake a cake"}
    assert all("list_id" not in i["data"] for i in got["changes"] if i["kind"] == "task")


async def test_config_flow(hass: HomeAssistant, hub_url):
    key = await new_account(hub_url)
    result = await hass.config_entries.flow.async_init(DOMAIN, context={"source": "user"})
    result = await hass.config_entries.flow.async_configure(
        result["flow_id"], {"hub": hub_url, "key": "AAAAA-BBBBB-CCCCC-DDDDD"})
    assert result["errors"] == {"key": "unknown_key"}
    result = await hass.config_entries.flow.async_configure(result["flow_id"], {"hub": hub_url, "key": "nope"})
    assert result["errors"] == {"key": "bad_key"}
    result = await hass.config_entries.flow.async_configure(result["flow_id"], {"hub": hub_url, "key": key.lower()})
    assert result["type"] == "create_entry"
    assert result["data"]["key"] == key
