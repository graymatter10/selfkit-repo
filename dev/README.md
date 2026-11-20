# Local fake server

`docker compose -f dev/docker-compose.yml up -d` starts:
- **Postgres 17** on `127.0.0.1:55432` (stand-in for Neon), data in Docker volume `selfkit-dev_pgdata` (tiny; /shared is NTFS and Postgres needs chmod).
- **SeaweedFS** S3 gateway on `127.0.0.1:59000` (stand-in for Backblaze B2; MinIO images are no longer public), data in `/shared/selfkitextra/s3`, creds in `s3.json`.

Both are memory-capped (256 MB / 384 MB). Credentials in `test.env` (dev only). The S3 tests create the bucket themselves.
Stop: `docker compose -f dev/docker-compose.yml down`. To switch to real Neon/B2, set the same env vars to real values (TLS is used automatically for `sslmode=require` URLs and `https://` endpoints).

# Laptop safety scripts

Run in the background while agents work (logs and the notify queue in `/shared/selfkitextra/locks/`; recreate that folder after a cleanup):

- `thermal-guard.sh`: build/test processes always at the lowest priority; paused in cycles from 65 °C (after 10 s) until 55 °C. Hard limit (user rule): CPU >= 70 °C for 40 s stops all build/test work; still >= 70 °C 40 s later saves uncommitted work as a local `refs/wip/thermal-*` snapshot and powers off.
- `power-guard.sh`: on battery below 30 % saves a `wip/power-*` snapshot (pushed if online), stops builds and powers off.
- `disk-guard.sh` (pauses work below 3 GB free, never deletes), `temp-log.sh` (temperature log).
- Every guard event is written to `locks/notify/` so the lead reports it in the main chat.

Start: `for s in thermal-guard power-guard disk-guard temp-log; do setsid nohup dev/$s.sh >/dev/null 2>&1 </dev/null & done`
