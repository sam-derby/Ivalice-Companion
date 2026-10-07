#!/usr/bin/env bash
set -euo pipefail

# Test resource reads through the same FUSE mount used by AppImages.
check_root=$(mktemp -d "$PWD/target/fuse-check.XXXXXX")
mkdir -p "$check_root/payload" "$check_root/mount"
printf '%s' 'synthetic bundled resource' > "$check_root/payload/fixture.bin"
mksquashfs "$check_root/payload" "$check_root/fixture.squashfs" -noappend -no-progress -processors 1
sudo squashfuse -o ro,allow_other "$check_root/fixture.squashfs" "$check_root/mount"
trap 'sudo umount "$check_root/mount"' EXIT
IVALICE_TEST_RESOURCE="$check_root/mount/fixture.bin" cargo test -p ivalice-infrastructure --locked --lib platform_fs::tests::read_only_fuse_resources_preserve_save_restrictions -- --ignored --exact
