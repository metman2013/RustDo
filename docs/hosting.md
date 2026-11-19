# Hosting the web app at rustdo.app

The web app is built by GitHub Actions and served by **GitHub Pages**. The domain
**rustdo.app** is registered at **Cloudflare**, which handles its DNS.

`.app` domains only work over HTTPS (browsers enforce this for the whole `.app` ending).
GitHub Pages issues the certificate automatically once DNS points at it.

Do these steps in order.

## 1. Let GitHub Actions publish Pages

GitHub → **RustDo** repo → **Settings → Pages** → under *Build and deployment*, set
**Source: GitHub Actions**.

The next push to `main` publishes the app. Until the domain is connected, it's at
`https://metman2013.github.io/RustDo/`.

## 2. Verify the domain with GitHub (recommended)

This stops anyone else's GitHub Pages site from claiming rustdo.app.

1. GitHub → your profile picture → **Settings → Pages** (account settings, not repo settings).
2. **Add a domain** → `rustdo.app` → GitHub shows a TXT record.
3. Add that TXT record in Cloudflare (step 3), then click **Verify** in GitHub.

## 3. DNS records in Cloudflare

Cloudflare dashboard → **rustdo.app** → **DNS → Records** → **Add record** for each row:

| Type | Name | Content | Proxy status |
|---|---|---|---|
| A | `@` | `185.199.108.153` | **DNS only** (grey cloud) |
| A | `@` | `185.199.109.153` | DNS only |
| A | `@` | `185.199.110.153` | DNS only |
| A | `@` | `185.199.111.153` | DNS only |
| AAAA | `@` | `2606:50c0:8000::153` | DNS only |
| AAAA | `@` | `2606:50c0:8001::153` | DNS only |
| AAAA | `@` | `2606:50c0:8002::153` | DNS only |
| AAAA | `@` | `2606:50c0:8003::153` | DNS only |
| CNAME | `www` | `metman2013.github.io` | DNS only |
| TXT | *(from step 2, e.g. `_github-pages-challenge-metman2013`)* | *(value GitHub shows)* | — |

**Keep the proxy off (grey cloud).** With Cloudflare's proxy on, GitHub can't issue the
HTTPS certificate. If you want Cloudflare's proxy later, turn it on only after step 5
works, and set Cloudflare **SSL/TLS** mode to **Full (strict)**.

Delete any other `A`, `AAAA` or `CNAME` records for `@` or `www`, such as parking-page
records Cloudflare may have added.

## 4. Connect the domain to the repo

GitHub → RustDo repo → **Settings → Pages → Custom domain** → type `rustdo.app` → **Save**.

GitHub runs a DNS check. That can take from a few minutes to an hour after adding the
records. `www.rustdo.app` automatically redirects to `rustdo.app`.

## 5. HTTPS

On the same Pages settings screen, wait until the certificate is issued (*"Enforce HTTPS"*
becomes clickable), then tick **Enforce HTTPS**. Open https://rustdo.app to check.

## Troubleshooting

- **"Domain's DNS record could not be retrieved"**: wait a bit longer, and check that every
  record in step 3 is set to **DNS only**.
- **Certificate stuck "pending"**: remove the custom domain in GitHub, save, add it again.
- **Old version showing**: the service worker updates on the next visit. Reload once more,
  or close and reopen the installed app.
