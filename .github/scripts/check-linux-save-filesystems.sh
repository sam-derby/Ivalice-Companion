#!/usr/bin/env bash
set -euo pipefail

# Explicit, privileged integration checks; never part of ordinary validation.
if [[ ${IVALICE_TEST_LOCAL_FILESYSTEMS:-} != 1 ]]; then
  echo 'Opt in with IVALICE_TEST_LOCAL_FILESYSTEMS=1 (disposable file-backed images only).'
  exit 2
fi
for tool in mkfs.ntfs ntfs-3g mkfs.exfat mount umount truncate cargo; do
  command -v "$tool" >/dev/null || { echo "Missing test prerequisite: $tool"; exit 2; }
done
check_root=$(mktemp -d "$PWD/target/save-filesystems.XXXXXX")
check_root=$(realpath "$check_root")
[[ $check_root == "$(realpath "$PWD/target")"/save-filesystems.* ]] || exit 2
test_uid=${SUDO_UID:-$(id -u)}
test_gid=${SUDO_GID:-$(id -g)}
if [[ $(id -u) == 0 ]]; then
  [[ $test_uid != 0 && -n ${SUDO_USER:-} ]] || { echo 'Run via sudo from an unprivileged test user.'; exit 2; }
  chown "$test_uid:$test_gid" "$check_root"
fi
run_test() {
  if [[ $(id -u) == 0 ]]; then
    runuser -u "$SUDO_USER" -- env "IVALICE_TEST_SAVE_DIRECTORY=$1" "IVALICE_TEST_DRIVER=$2" cargo test -p ivalice-infrastructure --locked --lib platform_fs::location_tests::mounted_save_publication -- --ignored --exact
  else
    IVALICE_TEST_SAVE_DIRECTORY="$1" IVALICE_TEST_DRIVER="$2" cargo test -p ivalice-infrastructure --locked --lib platform_fs::location_tests::mounted_save_publication -- --ignored --exact
  fi
}
cleanup() {
  for kind in ntfs exfat; do
    if mountpoint -q "$check_root/$kind"; then sudo umount "$check_root/$kind"; fi
  done
}
trap cleanup EXIT
for kind in ntfs exfat; do
  image="$check_root/$kind.img"
  mkdir "$check_root/$kind"
  # mktemp creates a fresh root. Refuse existing paths/links before formatting.
  [[ ! -e $image && ! -L $image ]] || exit 2
  truncate -s 128M "$image"
  [[ -f $image && ! -L $image && $(realpath "$image") == "$check_root/$kind.img" ]] || exit 2
  if [[ $kind == ntfs ]]; then
    mkfs.ntfs -F -Q -L IVCTEST "$image"
    sudo ntfs-3g "$image" "$check_root/$kind" -o "uid=$test_uid,gid=$test_gid,umask=0077"
  else
    mkfs.exfat -L IVCTEST "$image"
    sudo mount -t exfat -o "loop,uid=$test_uid,gid=$test_gid,umask=0077" "$image" "$check_root/$kind"
  fi
  if [[ $kind == ntfs ]]; then run_test "$check_root/$kind" ntfs-3g; else run_test "$check_root/$kind" exfat; fi
  if [[ $kind == exfat ]]; then
    if [[ $(id -u) == 0 ]]; then
      runuser -u "$SUDO_USER" -- env "IVALICE_TEST_SAVE_DIRECTORY=$check_root/$kind" cargo test -p ivalice-infrastructure --locked --lib save_edit::linux_tests::mounted_failed_promotion_recovers_original -- --ignored --exact
    else
      IVALICE_TEST_SAVE_DIRECTORY="$check_root/$kind" cargo test -p ivalice-infrastructure --locked --lib save_edit::linux_tests::mounted_failed_promotion_recovers_original -- --ignored --exact
    fi
    printf '%s' 'synthetic read-only save' > "$check_root/$kind/read-only.png"
    sudo mount -o remount,ro "$check_root/$kind"
    if [[ $(id -u) == 0 ]]; then
      runuser -u "$SUDO_USER" -- env "IVALICE_TEST_READ_ONLY_SAVE=$check_root/$kind/read-only.png" cargo test -p ivalice-infrastructure --locked --lib platform_fs::location_tests::mounted_save_read_only -- --ignored --exact
    else
      IVALICE_TEST_READ_ONLY_SAVE="$check_root/$kind/read-only.png" cargo test -p ivalice-infrastructure --locked --lib platform_fs::location_tests::mounted_save_read_only -- --ignored --exact
    fi
  fi
  sudo umount "$check_root/$kind"
done
echo 'NTFS-3G and native exFAT image checks passed. Native ntfs3 is not exercised by this script.'
