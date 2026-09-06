#!/bin/bash

set -xe

release_bin_path=target/x86_64-unknown-uefi/release/loader.efi
bootloader_name=BOOTx64.EFI

rm -rf disk/
mkdir -p disk/EFI/BOOT/
cp $release_bin_path disk/EFI/BOOT/$bootloader_name
cp stuff/kernel.bin disk/EFI/kernel.bin

rm -f disk.img
truncate -s 36M disk.img
mkfs.fat -F 32 disk.img
mcopy -i disk.img -s disk/* ::/
