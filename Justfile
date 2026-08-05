# Rebuild the assembly files, then the kernel as relocatable elf, then link the kernel
# and the loader.o to create the final image for grub.
dev: loader irq isr
    cargo build
    ld --script src/linker.ld build/loader.o target/i686-kernel/debug/frost -o build/kernel.bin
    cp build/kernel.bin build/iso/boot
    grub-mkrescue -o build/frost.iso build/iso

# Same as `dev` above, just use `--release` for the kernel.
release: loader irq isr
    cargo build --release
    ld --script src/linker.ld build/loader.o target/i686-kernel/release/frost -o build/kernel.bin
    cp build/kernel.bin build/iso/boot
    grub-mkrescue -o build/frost.iso build/iso

# Build the `loader.o` relocatable object file.
loader:
    nasm -felf32 src/loader.asm -o build/loader.o

# Build `irq.o` relocatable object file.
irq:
    nasm -felf32 src/irq.asm -o build/irq.o

# Build `isr.o` relocatable object file.
isr:
    nasm -felf32 src/isr.asm -o build/isr.o

# Only build the grub disk image from the linked kernel.
grub:
    cp build/kernel.bin build/iso/boot
    grub-mkrescue -o build/frost.iso build/iso

# Build(debug) and run.
full: dev run

# Run QEMU
run:
    qemu-system-i386 -cdrom build/frost.iso -serial stdio -no-reboot -no-shutdown \
        -d int,cpu_reset -net none -m 128 -smp 4

clean:
    rm build/loader.o build/kernel.bin build/frost.iso build/isr.o build/irq.o \
        build/iso/boot/kernel.bin
