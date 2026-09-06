#!/bin/bash

set -xe

BUILD=build

qemu-system-i386 \
    -net none \
    -drive file=$BUILD/disk.img,format=raw \
    -drive if=pflash,format=raw,unit=0,file=firmwares/x86/OVMF32_CODE_4M.fd,readonly=on \
    -m 128M \
    -smp 4 \
    --display none \
    --machine pc-q35-10.0,spcr=off \
    --serial stdio

