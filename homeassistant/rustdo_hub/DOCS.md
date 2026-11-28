# RustDo Hub

RustDo Hub keeps RustDo in sync on all your devices (Settings › **Sync my devices**) and
lets you share lists such as groceries or chores with your family (**Family sharing**).
Nothing goes through anyone's cloud: your data stays on this Home Assistant machine.

## Set up

1. Start this app.
2. In RustDo, open **Settings › Sync my devices › Turn on sync**, and enter your Home
   Assistant's address with port **7979**, for example `homeassistant.local:7979` or
   `192.168.1.20:7979`. Choose **This is my first device**; RustDo shows a sync key.
3. On your other devices, turn on sync with the same address and choose **I already have a
   sync key**.

To share a list with your family, right-click (long-press on a phone) a list in RustDo,
choose **Share list**, and enter the same address.

## Your lists in Home Assistant

The **RustDo** integration shows your RustDo lists as Home Assistant to-do lists
(dashboards, voice, automations). Install it with HACS and connect it to
`localhost:7979` with your sync key: see
[RustDo in Home Assistant](https://github.com/metman2013/RustDo/blob/main/docs/home-assistant.md).

The **Open web UI** button shows the hub's status page (version and counts only, never
your lists).

## Port

The hub listens on port **7979**. To use another port, change it in this app's
**Network** settings and use that port in RustDo.

## Data and backups

The hub's data lives in this app's storage and is included in Home Assistant's own
backups. The hub also keeps a daily copy of the last 14 days in its `backups` folder.

## The web app and away from home

The web app at rustdo.app needs the hub on an `https://` address. If you already reach
Home Assistant from outside over HTTPS (for example with a reverse proxy on your own
domain), point a subdomain at port 7979 of this machine the same way. See the
[hub guide](https://github.com/metman2013/RustDo/blob/main/docs/hub.md) for details.
Don't forward port 7979 on your router directly.
