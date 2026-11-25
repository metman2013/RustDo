# RustDo Hub: share lists with family and friends

RustDo Hub is a tiny program you run at home: on a PC, a NAS, a Raspberry Pi or any
machine with Docker. RustDo apps on your network use it to keep **shared lists** in sync.
Groceries, chores, a holiday packing list: everyone sees the same list and changes show
up on the other devices within about 30 seconds.

- **No cloud, no accounts.** Shared lists are stored only on your hub, in one file.
- **Only the lists you share.** Every other list stays on your own device.
- **My Day stays personal.** Adding a shared task to *your* My Day doesn't put it in
  anyone else's.
- Works in the **Windows, Linux and Android** apps. (The web app at rustdo.app can't: a
  secure web page isn't allowed to talk to a plain address on your home network.)

## 1. Start a hub

Pick one machine that's usually on. You only need one hub for the whole family.

### Docker (NAS, home server, Raspberry Pi, any Linux box)

```bash
docker run -d --name rustdo-hub --init --restart unless-stopped -p 7979:7979 -v rustdo-hub-data:/data ghcr.io/metman2013/rustdo-hub:latest
```

Or with Docker Compose, using [hub/docker-compose.yml](../hub/docker-compose.yml):

```bash
docker compose up -d
```

- Shared lists are kept in the `rustdo-hub-data` volume (`/data/hub.json` inside the container).
- To use another port, change the left side of the port mapping, e.g. `-p 8080:7979`.
- To update: `docker pull ghcr.io/metman2013/rustdo-hub:latest`, then remove the
  container and run it again. Your data is in the volume, so nothing is lost.
- Images are published for `amd64` (PCs and most NAS boxes) and `arm64` (Raspberry Pi 4/5
  and ARM NAS boxes). Prefer to build it yourself? Run `docker build -t rustdo-hub hub/` from
  the repository folder.

### Windows

1. Download `RustDo-Hub-<version>-windows.exe` from the
   [Releases page](https://github.com/metman2013/RustDo/releases).
2. Double-click it. A window opens and shows the **hub address**, e.g. `http://192.168.1.20:7979`.
3. When Windows Firewall asks, allow it on **Private networks**. Phones and other PCs can't
   reach the hub otherwise.
4. Leave the window open (minimize it). Closing it stops the hub.

Shared lists are stored in `%APPDATA%\RustDo Hub\hub.json`.

**Start it with Windows:** press `Win + R`, type `shell:startup`, press Enter, and put a
shortcut to the exe in that folder. In the shortcut's properties, set **Run** to
*Minimized*.

### Linux (without Docker)

Build it with `cargo build --release` in the `hub/` folder, then run
`hub/target/release/rustdo-hub`. Shared lists are stored in `./data`. You can change
that with `--data <folder>`, and the port with `--port <n>`.

## 2. Share a list

1. In RustDo, right-click the list (on a phone, long-press it), then choose **👥 Share list…**.
2. Enter the hub address. Just the IP is fine, e.g. `192.168.1.20`; port 7979 is assumed.
3. Press **Share**. You get a code like `K7QM-2XPD`. Send it to the people you're
   sharing with.

Shared lists show a 👥 icon. To see the code again, right-click the list and choose **👥 Sharing…**.

## 3. Join a shared list

**⚙ → Family sharing → Join a shared list…**, enter the hub address and the code. The
list appears in your lists and stays in sync from then on.

## How syncing works

- RustDo syncs every 30 seconds while it's open, and a couple of seconds after you change
  something. **⚙ → Family sharing → Sync now** syncs straight away.
- Each task syncs on its own. If two people change the **same task** at about the same
  time, the later change wins. Changes to *different* tasks never conflict.
- Away from home, or the hub is off? RustDo keeps working normally and catches up the next
  time it can reach the hub.
- **Deleting** a shared list in RustDo removes it from that device only; everyone else
  keeps it. **Stop syncing on this device** (in the 👥 Sharing dialog) keeps your copy as a
  normal list.
- Moving a task out of a shared list removes it from the shared list for everyone.

## Using it away from home

The hub only answers on your home network. To use it from anywhere, without opening your
router to the internet, install [Tailscale](https://tailscale.com) (free for personal use)
on the hub machine and on each phone or PC. Then use the hub machine's Tailscale address
(`100.x.y.z:7979`) in RustDo. Your lists still never pass through anyone's cloud storage.

### Your own subdomain behind a reverse proxy

Already run a reverse proxy with HTTPS (Synology's built-in one, Nginx Proxy Manager,
Caddy, Traefik, nginx)? Point a subdomain such as `todo.example.com` at the hub, then
enter `https://todo.example.com` as the hub address in RustDo. Any path works the same
behind the proxy; the hub needs no extra settings.

- **Synology:** Control Panel › Login Portal › Advanced › **Reverse Proxy** › Create.
  Source: HTTPS, your subdomain, port 443. Destination: HTTP, `localhost`, port 7979 (the
  container's published port). Add a certificate for the subdomain under Control Panel ›
  Security › Certificate (Let's Encrypt works).
- **Caddy** (gets the certificate itself):

  ```
  todo.example.com {
      reverse_proxy localhost:7979
  }
  ```
- **nginx:** a `server` block for the subdomain with your certificate and
  `location / { proxy_pass http://127.0.0.1:7979; }`.

The share code is still the only key to a list, so keep the proxy on HTTPS (never plain
HTTP over the internet) and don't post codes publicly.

**Don't** forward port 7979 on your router: anyone on the internet who guessed a code could
then reach your lists, and the hub has no encryption of its own.

## Privacy and security

- The share code is the only key to a list. Anyone who has the code **and** can reach the
  hub can read and change that list. Codes are 8 random characters, about a trillion
  possibilities.
- The hub's status page (`http://<hub>:7979/` in a browser) shows only its version and how
  many lists are shared. It never shows list names, tasks or codes.
- Back up the hub by copying `hub.json` (or the Docker volume). If a hub's data is lost,
  share the lists again: every device still has its own full copy.

## Troubleshooting

| Problem | Fix |
|---|---|
| "Can't reach the hub" | Open `http://<hub address>:7979/` in the phone's or PC's browser. If the page doesn't load, the device is on another network (guest Wi-Fi, mobile data), or a firewall is blocking port 7979. |
| "No shared list has that code" | Check the code. It ignores case and dashes. Was the hub's data reset? Then share the list again. |
| Windows hub works only on the same PC | Allow `rustdo-hub.exe` on Private networks: Windows Security → Firewall → Allow an app. Also check your Wi-Fi is set to *Private*, not *Public*. |
| `docker pull` says "denied" | The image isn't public yet. The repository owner needs to set it to public once on GitHub: Packages → rustdo-hub → Package settings → Change visibility. |
