#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

grep -Fq 'icons/NOTICE' "$root/scripts/release/package.sh"
grep -Fq 'ICONSAX-NOTICE' "$root/scripts/release/package.sh"
grep -Fq 'Iconsax Free License' "$root/icons/NOTICE"
