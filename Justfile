build: loader irq isr gdt
    cargo build
    ld --script src/linker.ld build/loader.o build/irq.o build/isr.o \
        build/gdt.o target/i686-kernel/debug/libfrost.a -o build/kernel.bin
    cp build/kernel.bin build/iso/boot
    grub-mkrescue -o build/frost.iso build/iso

release: loader irq isr gdt
    cargo build --release
    ld --script src/linker.ld build/loader.o build/irq.o build/isr.o \
        build/gdt.o target/i686-kernel/release/libfrost.a -o build/kernel.bin
    cp build/kernel.bin build/iso/boot
    grub-mkrescue -o build/frost.iso build/iso

loader:
    nasm -felf32 src/loader.asm -o build/loader.o

irq:
    nasm -felf32 src/irq.asm -o build/irq.o

isr:
    nasm -felf32 src/isr.asm -o build/isr.o

gdt:
    nasm -felf32 src/gdt.asm -o build/gdt.o

# Build and run.
full: build run

run:
    qemu-system-i386 -cdrom build/frost.iso -serial stdio -no-reboot -no-shutdown \
        -d int,cpu_reset -net none

clean:
    rm build/loader.o build/kernel.bin build/frost.iso build/isr.o build/irq.o \
        build/gdt.o build/iso/boot/kernel.bin

quit:
    pkill -f qemu-system-i386
