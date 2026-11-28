# RustDo in Home Assistant

Your RustDo lists as Home Assistant **to-do lists**: on dashboards (the To-do list card),
with voice ("add milk to my groceries list"), and in automations (`todo.add_item`,
`todo.update_item`, `todo.remove_item`, `todo.get_items`). Changes go both ways through
your own RustDo Hub, the same way RustDo's devices keep each other in sync. No cloud.

You need:

- a **RustDo Hub** (the [Home Assistant app](../homeassistant/rustdo_hub/DOCS.md), or one
  on another machine: see the [hub guide](hub.md)), version 0.4.0 or later;
- **Sync my devices** turned on in RustDo with that hub (Settings › Sync & sharing).

## Install

With [HACS](https://hacs.xyz):

1. HACS › ⋮ › **Custom repositories**: add `https://github.com/metman2013/RustDo`,
   type **Integration**.
2. Find **RustDo** in HACS and download it, then restart Home Assistant.

Without HACS: copy the `custom_components/rustdo` folder from this repository into your
Home Assistant's `config/custom_components/` folder and restart Home Assistant.

## Set up

1. In RustDo, open **Settings › Sync & sharing** and copy your **sync key**
   (it looks like `ABCDE-FGHJK-MNPQR-STUVW`).
2. In Home Assistant: **Settings › Devices & services › Add integration › RustDo**.
3. Enter the hub's address, as RustDo uses it (for the Home Assistant app:
   `localhost:7979` or `homeassistant.local:7979`), and the sync key.

Each RustDo list becomes a to-do list entity, such as `todo.rustdo_groceries`. Lists you
add in RustDo later appear by themselves. Family-shared lists you're in show up too (with
a people icon), as long as Home Assistant can reach the hub they're shared through.

## What works

| In Home Assistant | In RustDo |
| --- | --- |
| Add an item (with a due date and description) | A new task, with its due date and note |
| Tick an item off | The task is completed; a repeating task gets its next one, as in RustDo |
| Rename, change the due date or description | The task's title, due date or note |
| Delete an item | The task is deleted |

Changes from RustDo show up in Home Assistant within 30 seconds. Steps (subtasks),
reminders, My Day and importance stay in RustDo: Home Assistant's to-do lists don't have
them, and editing an item there leaves them as they are.

## Keep the sync key private

Anyone with the sync key who can reach your hub can see and change your tasks, so treat it
like a password. If you turn sync off and on again in RustDo with a new key, Home Assistant
asks for the new one.
