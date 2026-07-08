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
    as --32 src/loader.S -o build/loader.o

irq:
    as --32 src/irq.S -o build/irq.o

isr:
    as --32 src/isr.S -o build/isr.o

gdt:
    as --32 src/gdt.S -o build/gdt.o

run: build
    qemu-system-i386 -cdrom build/frost.iso -serial stdio -no-reboot -no-shutdown \
        -d int,cpu_reset

clean:
    rm build/loader.o build/kernel.bin build/frost.iso build/isr.o build/irq.o \
        build/gdt.o build/iso/boot/kernel.bin

quit:
    pkill -f qemu-system-i386
