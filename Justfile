[working-directory: 'kernel']
kernel:
    cargo build --release

[working-directory: 'kernel']
dev-kernel:
    cargo build

[working-directory: 'bootloader']
uefi:
    cargo build --release

[working-directory: 'bootloader']
dev-uefi:
    cargo build

build: kernel uefi

dev-build: dev-kernel dev-uefi

run:
    ./scripts/build-ia32.sh
    ./scripts/qemu-ia32.sh

full: build run
