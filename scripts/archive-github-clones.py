#!/usr/bin/env python3
"""Archive GitHub repository clone traffic beyond GitHub's 14-day window."""
import csv
import json
import os
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

owner = "vikash7644"
repo = "zenvora-lang"
token = os.environ["TRAFFIC_TOKEN"]
url = f"https://api.github.com/repos/{owner}/{repo}/traffic/clones"
request = urllib.request.Request(
    url,
    headers={
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "qorvynt-clone-traffic-archiver",
    },
)
with urllib.request.urlopen(request, timeout=30) as response:
    payload = json.load(response)

path = Path("traffic/clones.csv")
path.parent.mkdir(parents=True, exist_ok=True)
records = {}
if path.exists():
    with path.open(newline="", encoding="utf-8") as file:
        for row in csv.DictReader(file):
            records[row["date"]] = row

# GitHub returns a rolling daily series (up to the last 14 days).
for item in payload.get("clones", []):
    day = datetime.fromtimestamp(item["timestamp"], timezone.utc).date().isoformat()
    records[day] = {
        "date": day,
        "clones": str(item["count"]),
        "unique_cloners": str(item["uniques"]),
    }

with path.open("w", newline="", encoding="utf-8") as file:
    writer = csv.DictWriter(file, fieldnames=["date", "clones", "unique_cloners"])
    writer.writeheader()
    writer.writerows(records[key] for key in sorted(records))

print(f"Archived {len(payload.get('clones', []))} daily points; {len(records)} dates stored.")
