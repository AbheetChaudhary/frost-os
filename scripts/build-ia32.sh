#!/bin/bash

BUILD=build

set -xe

bootloader_release_bin_path=bootloader/target/i686-unknown-uefi/release/loader.efi
bootloader_name=BOOTIA32.EFI

kernel_release_bin_path=kernel/target/i686-kernel/release/frost

rm -rf $BUILD/disk/
mkdir -p $BUILD/disk/EFI/BOOT/
cp $bootloader_release_bin_path $BUILD/disk/EFI/BOOT/$bootloader_name
cp $kernel_release_bin_path $BUILD/disk/EFI/kernel.bin

rm -f $BUILD/disk.img
truncate -s 36M $BUILD/disk.img
mkfs.fat -F 32 $BUILD/disk.img
mcopy -i $BUILD/disk.img -s $BUILD/disk/* ::/
