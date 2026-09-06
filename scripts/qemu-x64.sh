#!/bin/bash

set -xe

qemu-system-x86_64 \
    -net none \
    -bios /usr/share/ovmf/x64/OVMF.4m.fd \
    -drive format=raw,file=disk.img \
    -m 128M \
    -smp 4 \
    --display none \
    --serial stdio

