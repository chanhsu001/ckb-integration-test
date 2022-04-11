# CKB Sync-Mainnet Test

This job will compile ckb locally, please use `ubuntu-focal-20.04-amd64` to run the scripts.

Required environment variables:

- `AWS_ACCESS_KEY`
- `AWS_SECRET_KEY`
- `GITHUB_TOKEN`

Usage:

```
# Start a sync-mainnet test
./script/sync-mainnet.sh run

# Clean environment
./script/sync-mainnet.sh clean
```
