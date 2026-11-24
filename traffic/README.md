# GitHub clone traffic archive

The `clones.csv` file stores daily clone counts and unique cloners collected from GitHub's repository traffic API.

## One-time setup

1. Create a fine-grained personal access token for **vikash7644/zenvora-lang**.
2. Grant **Repository permissions → Administration: Read-only**. Do not grant write access to Administration.
3. In this repository, open **Settings → Secrets and variables → Actions → New repository secret**.
4. Name the secret `TRAFFIC_TOKEN` and paste the token value.

The workflow runs daily at 00:17 UTC and can also be started manually from **Actions → Archive GitHub clone traffic → Run workflow**.

On its first successful run, it imports the daily points still available in GitHub's rolling 14-day window. Later runs keep appending/updating daily records in `clones.csv`; previous dates are not discarded.

**Limitations:** GitHub only exposes a rolling 14-day traffic window, so dates before the first successful archive run cannot be recovered by this workflow. Counts are repository clone events, not necessarily distinct people or completed unique device installs.
