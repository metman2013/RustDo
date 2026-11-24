#!/usr/bin/env bash
# Posts "RustDo <version> is out" with the release notes to a Discord channel.
#
#   DISCORD_WEBHOOK_URL=... tools/announce-discord.sh <version> <notes.md> <release-url>
#
# Called by the release job in .github/workflows/build.yml. Without DISCORD_WEBHOOK_URL it
# does nothing; with DRY_RUN=1 it prints the message instead of posting it.
# Discord: channel > Edit Channel > Integrations > Webhooks > New Webhook > Copy Webhook URL.
set -euo pipefail
version="$1"; notes="$2"; url="$3"

if [ -z "${DISCORD_WEBHOOK_URL:-}" ] && [ -z "${DRY_RUN:-}" ]; then
  echo "No DISCORD_WEBHOOK_URL secret: not announcing on Discord."
  exit 0
fi

# CHANGELOG headings ("### New") become bold lines; Discord embeds allow 4096 characters.
body=$(sed -E 's/^### (.*)$/**\1**/' "$notes" | sed -e '/./,$!d' | head -c 3800)
body="$body

**Download:** $url"

payload=$(jq -n \
  --arg title "RustDo $version is out" \
  --arg url "$url" \
  --arg desc "$body" \
  '{username: "RustDo", embeds: [{title: $title, url: $url, description: $desc, color: 3900150}]}')

if [ -n "${DRY_RUN:-}" ]; then
  echo "$payload"
  exit 0
fi
curl -fsS --retry 3 -H "Content-Type: application/json" -d "$payload" "$DISCORD_WEBHOOK_URL" >/dev/null
echo "Announced RustDo $version on Discord."
