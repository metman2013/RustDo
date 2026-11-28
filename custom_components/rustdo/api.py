"""Talks to a RustDo Hub the way a RustDo device does.

The integration joins "Sync my devices" with the person's sync key, so it sees their lists
and tasks, and it follows the family-shared lists they're in through each list's share code.
Changes go to the hub as items with a timestamp; the newest edit of an item wins, as between
RustDo's own devices. Nothing here depends on Home Assistant, so it can be tested alone.
"""

from __future__ import annotations

import calendar
import time
import uuid
from dataclasses import dataclass, field
from datetime import date, datetime, timedelta
from typing import Any

import aiohttp


class HubError(Exception):
    """The hub couldn't be reached or refused the request."""


class BadKey(HubError):
    """The hub doesn't know this sync key."""


def normalize_hub(text: str) -> str | None:
    """What people type (`192.168.1.20`, `nas.local:7979`, `https://todo.example.com/`)."""
    s = text.strip().rstrip("/")
    if not s or any(c.isspace() for c in s):
        return None
    if not s.startswith(("http://", "https://")):
        s = "http://" + s
    rest = s.split("://", 1)[1]
    if ":" not in rest and s.startswith("http://"):
        s += ":7979"
    return s


def normalize_key(text: str) -> str | None:
    """"abcde fghjk…" → "ABCDE-FGHJK-MNPQR-STUVW"; None unless it has 20 letters and digits."""
    c = "".join(ch for ch in text if ch.isascii() and ch.isalnum()).upper()
    if len(c) != 20:
        return None
    return "-".join(c[i : i + 5] for i in range(0, 20, 5))


def now_ms() -> int:
    return int(time.time() * 1000)


def naive_now() -> str:
    """Local time as RustDo writes it (no time zone): 2026-11-28T17:03:06.723."""
    return datetime.now().strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3]


@dataclass
class TaskView:
    """A task as Home Assistant shows it."""

    id: str
    title: str
    done: bool
    notes: str
    due: date | None
    data: dict[str, Any]


@dataclass
class ListView:
    id: str
    name: str
    pos: int
    # None: one of the person's own lists (through the account); else the share code.
    code: str | None
    tasks: list[TaskView] = field(default_factory=list)


@dataclass
class Share:
    hub: str
    code: str
    list_id: str | None = None
    rev: int = 0
    # item id -> item
    items: dict[str, dict[str, Any]] = field(default_factory=dict)


class RustDo:
    """The person's RustDo data as the hub has it, kept up to date by `refresh`."""

    def __init__(self, session: aiohttp.ClientSession, hub: str, key: str) -> None:
        self.session = session
        self.hub = hub
        self.key = key
        self.rev = 0
        # "kind:id" -> item
        self.items: dict[str, dict[str, Any]] = {}
        self.shares: dict[str, Share] = {}

    # ------------------------------------------------------------------ network

    async def _post(self, hub: str, path: str, body: dict[str, Any]) -> dict[str, Any]:
        try:
            async with self.session.post(
                f"{hub}{path}", json=body, timeout=aiohttp.ClientTimeout(total=20)
            ) as r:
                try:
                    data = await r.json(content_type=None)
                except ValueError as e:
                    raise HubError(f"unexpected answer from the hub ({r.status})") from e
                if r.status >= 400:
                    message = (data or {}).get("error") if isinstance(data, dict) else None
                    if r.status == 404 and message == "not found":
                        raise HubError("this RustDo Hub is too old: update it to 0.4.0 or later")
                    if r.status == 404 and path.startswith("/api/account"):
                        raise BadKey(message or "the hub doesn't know this sync key")
                    raise HubError(message or f"the hub refused the request ({r.status})")
                return data
        except (aiohttp.ClientError, TimeoutError) as e:
            raise HubError(f"can't reach the hub at {hub}: {e}") from e

    async def _account_round(self, changes: list[dict[str, Any]]) -> None:
        resp = await self._post(
            self.hub, "/api/account/sync", {"key": self.key, "since": self.rev, "changes": changes}
        )
        for item in resp.get("changes", []):
            k = f"{item['kind']}:{item['id']}"
            if item.get("deleted"):
                self.items.pop(k, None)
            else:
                self.items[k] = item
        self.rev = resp.get("rev", self.rev)

    async def _share_round(self, share: Share, changes: list[dict[str, Any]]) -> None:
        resp = await self._post(
            share.hub, "/api/sync", {"code": share.code, "since": share.rev, "changes": changes}
        )
        share.list_id = resp.get("list_id", share.list_id)
        for item in resp.get("changes", []):
            if item.get("deleted"):
                share.items.pop(item["id"], None)
            else:
                share.items[item["id"]] = item
        share.rev = resp.get("rev", share.rev)

    async def refresh(self) -> None:
        """Catches up with the hub: the account, then each shared list the person is in."""
        await self._account_round([])
        wanted: dict[str, str] = {}
        for item in self.items.values():
            if item["kind"] == "share":
                d = item.get("data") or {}
                if d.get("code"):
                    wanted[d["code"]] = d.get("hub") or self.hub
        for code in list(self.shares):
            if code not in wanted:
                del self.shares[code]
        errors = []
        for code, hub in wanted.items():
            share = self.shares.setdefault(code, Share(hub=hub, code=code))
            share.hub = hub
            try:
                await self._share_round(share, [])
            except HubError as e:
                # One unreachable shared list doesn't hide the others.
                errors.append(e)
        if errors and len(errors) == len(wanted) and not any(
            i["kind"] == "list" for i in self.items.values()
        ):
            raise errors[0]

    # ------------------------------------------------------------------ reading

    def lists(self) -> list[ListView]:
        out: dict[str, ListView] = {}
        for item in self.items.values():
            if item["kind"] == "list":
                d = item.get("data") or {}
                out[item["id"]] = ListView(item["id"], d.get("name", ""), d.get("pos", 0), None)
        for item in self.items.values():
            if item["kind"] == "task":
                d = item.get("data") or {}
                lv = out.get(d.get("list_id"))
                if lv is not None:
                    lv.tasks.append(_task_view(item["id"], d))
        for share in self.shares.values():
            if not share.list_id:
                continue
            name = next(
                (i["data"].get("name", "") for i in share.items.values() if i["kind"] == "list"),
                "",
            )
            lv = ListView(share.list_id, name, 10_000, share.code)
            lv.tasks = [_task_view(i["id"], i.get("data") or {}) for i in share.items.values() if i["kind"] == "task"]
            out[share.list_id] = lv
        for lv in out.values():
            # Open tasks newest first, then completed ones (as RustDo's "Newest first").
            lv.tasks.sort(key=lambda t: (t.done, _neg(t.data.get("created") or "")))
        return sorted(out.values(), key=lambda lv: (lv.pos, lv.name.lower()))

    def find_list(self, list_id: str) -> ListView | None:
        return next((lv for lv in self.lists() if lv.id == list_id), None)

    # ------------------------------------------------------------------ writing

    async def _send(self, lv: ListView, task_id: str, data: dict[str, Any] | None) -> None:
        """Sends one task (or its deletion, with `data` None) to wherever its list lives."""
        item = {"kind": "task", "id": task_id, "modified": now_ms(), "deleted": data is None, "data": data, "rev": 0}
        if lv.code is None:
            await self._account_round([item])
        else:
            await self._share_round(self.shares[lv.code], [item])

    def _task_data(self, lv: ListView, task_id: str) -> dict[str, Any] | None:
        if lv.code is None:
            item = self.items.get(f"task:{task_id}")
        else:
            item = self.shares[lv.code].items.get(task_id)
        return dict(item["data"]) if item and item.get("data") else None

    async def add_task(self, list_id: str, title: str, due: date | None = None, notes: str = "") -> str:
        lv = self.find_list(list_id)
        if lv is None:
            raise HubError("this list isn't in RustDo any more")
        task_id = str(uuid.uuid4())
        data: dict[str, Any] = {
            "title": title.strip(),
            "notes": notes or "",
            "done": False,
            "important": False,
            "due": due.isoformat() if due else None,
            "reminder": None,
            "recurrence": "Never",
            "steps": [],
            "created": naive_now(),
            "completed_at": None,
        }
        if lv.code is None:
            data["list_id"] = list_id
            data["my_day"] = None
        await self._send(lv, task_id, data)
        return task_id

    async def update_task(
        self,
        list_id: str,
        task_id: str,
        *,
        title: str | None = None,
        done: bool | None = None,
        due: date | None | type[...] = ...,
        notes: str | None = None,
    ) -> None:
        lv = self.find_list(list_id)
        data = self._task_data(lv, task_id) if lv else None
        if lv is None or data is None:
            raise HubError("this task isn't in RustDo any more")
        if title is not None:
            data["title"] = title.strip()
        if notes is not None:
            data["notes"] = notes
        if due is not ...:
            data["due"] = due.isoformat() if due else None
        next_task: tuple[str, dict[str, Any]] | None = None
        if done is not None and bool(data.get("done")) != done:
            data["done"] = done
            data["completed_at"] = naive_now() if done else None
            if done:
                next_task = _next_occurrence(data)
                if next_task is not None:
                    # The completed one stops repeating, as in RustDo.
                    data["recurrence"] = "Never"
        await self._send(lv, task_id, data)
        if next_task is not None:
            await self._send(lv, next_task[0], next_task[1])

    async def delete_tasks(self, list_id: str, task_ids: list[str]) -> None:
        lv = self.find_list(list_id)
        if lv is None:
            return
        items = [
            {"kind": "task", "id": t, "modified": now_ms(), "deleted": True, "data": None, "rev": 0}
            for t in task_ids
        ]
        if lv.code is None:
            await self._account_round(items)
        else:
            await self._share_round(self.shares[lv.code], items)


def _neg(s: str) -> str:
    """Sorts text in reverse inside an ascending sort."""
    return "".join(chr(0x10FFFF - ord(c)) for c in s)


def _task_view(task_id: str, d: dict[str, Any]) -> TaskView:
    due = None
    if d.get("due"):
        try:
            due = date.fromisoformat(d["due"])
        except ValueError:
            due = None
    return TaskView(task_id, d.get("title", ""), bool(d.get("done")), d.get("notes") or "", due, d)


# ---------------------------------------------------------------------- repeats


def _add_months(d: date, months: int) -> date:
    m = d.month - 1 + months
    y = d.year + m // 12
    m = m % 12 + 1
    return date(y, m, min(d.day, calendar.monthrange(y, m)[1]))


def next_date(recurrence: str, start: date, every: int = 1) -> date | None:
    """RustDo's Recurrence::next_after_every."""
    n = max(every or 1, 1)
    if recurrence == "Daily":
        return start + timedelta(days=n)
    if recurrence == "Weekly":
        return start + timedelta(weeks=n)
    if recurrence == "Monthly":
        return _add_months(start, n)
    if recurrence == "Yearly":
        return _add_months(start, 12 * n)
    if recurrence == "Weekdays":
        d = start + timedelta(days=1)
        while d.weekday() >= 5:
            d += timedelta(days=1)
        return d
    return None


def _next_occurrence(data: dict[str, Any]) -> tuple[str, dict[str, Any]] | None:
    """Completing a repeating task makes its next one (as RustDo's Store::set_done)."""
    recurrence = data.get("recurrence") or "Never"
    if recurrence == "Never":
        return None
    planned = data.get("due") or (data.get("reminder") or "")[:10] or None
    base = date.fromisoformat(planned) if planned else date.today()
    nxt = next_date(recurrence, base, int(data.get("repeat_every") or 1))
    if nxt is None:
        return None
    shift = nxt - base
    new = dict(data)
    new.update(done=False, completed_at=None, created=naive_now())
    if "my_day" in new:
        new["my_day"] = None
    if new.get("due"):
        new["due"] = nxt.isoformat()
    if new.get("reminder"):
        try:
            r = datetime.fromisoformat(new["reminder"]) + shift
            new["reminder"] = r.strftime("%Y-%m-%dT%H:%M:%S")
        except ValueError:
            pass
    new["steps"] = [dict(s, id=str(uuid.uuid4()), done=False) for s in new.get("steps") or []]
    return str(uuid.uuid4()), new
