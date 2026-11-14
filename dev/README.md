# Local fake server

`docker compose -f dev/docker-compose.yml up -d` starts:
- **Postgres 17** on `127.0.0.1:55432` (stand-in for Neon), data in Docker volume `selfkit-dev_pgdata` (tiny; /shared is NTFS and Postgres needs chmod).
- **SeaweedFS** S3 gateway on `127.0.0.1:59000` (stand-in for Backblaze B2; MinIO images are no longer public), data in `/shared/selfkitextra/s3`, creds in `s3.json`.

Both are memory-capped (256 MB / 384 MB). Credentials in `test.env` (dev only). The S3 tests create the bucket themselves.
Stop: `docker compose -f dev/docker-compose.yml down`. To switch to real Neon/B2, set the same env vars to real values (TLS is used automatically for `sslmode=require` URLs and `https://` endpoints).
