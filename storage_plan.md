# Gaia DR3 XP continuous bulk storage plan

**Conclusion:** PASS

## Population

- XP continuous-only sources: 184729270
- Official bulk files: 3386
- Official compressed volume: 3.3 TiB

## Disk measurements

| Volume | Available | Existing measured |
| --- | --- | --- |
| internal (`/home/valles/nsb-data/starlight-gaia-release/work`) | 41.09 GiB | 128.93 GiB |
| USB cache | 223.41 GiB | n/a |

## Rotating cache

- Max cache bytes: 20.00 GiB
- Max USB file bytes: 3.73 GiB
- Max observed file bytes: 1.57 GiB
- Files over USB limit: 0
- Peak rotating cache bytes: 20.00 GiB

## Feasibility

- Can process full population: true
- Internal headroom: 36.89 GiB
- USB headroom: 203.41 GiB
- Rationale:
  - USB rotating cache fits within 21474836480 byte ceiling
  - internal work/checkpoint/output headroom: 39610585907 bytes
  - 184,729,270 XP continuous-only sources are feasible with rotating USB cache

## Preflight gates

| Gate | Status | Detail |
| --- | --- | --- |
| complete_official_inventory | PASS | inventory_files=3386 expected=3386 files_over_usb_limit=0 |
| checksums | PASS | official manifest sha256:f23df1ffb45b19fc3f34d6f37791179cef1ebec6c5b9fd613a488b3be580fccd |
| file_size_limit | PASS | max_observed_bytes=1690137459 limit=4000000000 |
| policy_checksum | PASS | phase5_frozen_validation_policy_v1.json sha256:c525de3ec6d0022a6ed468f8f2bde2515e8f8364915f5a7a02492eee21947b74 |
| usb_mount_identity | PASS | mount=/media/valles/RAMONJR uuid=000000000000000018c1839a0bea693f |
| cleanup_simulation | PASS | dry_run candidates=4 bytes_reclaimed=6540796676 |
| usb_cache_state | PASS | footprint_bytes=6540796676 max_cache_bytes=21474836480 |
| storage_plan | PASS | rotating USB cache and internal work volumes fit required peak footprint |
