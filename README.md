## Info
The kernel is statically compiled into a relocatable elf binary. It has some
dependencies such as `irq.o` and `isr.o` which are build from their respective
assembly files and linked in the relocatable kernel binary.

The binary that `cargo` produces is then linked with a `multiboot` stub from
`loader.asm` file to prepare it for grub, thus giving the final `kernel.bin`.

The `kernel.bin` is then packed into an iso file by putting it inside an
appropriate directory tree and running `grub-mkrescue` on it.

The final iso could then be booted using QEMU. There are helper `just` commands
to do various tasks.

## Requirements
- Rust nightly toolchain
- NASM assembler
- Just, the command runner
- QEMU, specifically qemu-system-i386

## `Just` commands for running.
- `just` to build the qemu bootable kernel.
- `just run` to build the kernel and run it in qemu.
- `just clean` to clean intermediate object files, use `cargo clean` to clean the
  rust code as well.
- `just release` to do a release build.

## Resources
- [Brandon F.'s OSDev blog](http://www.osdever.net/bkerndev/Docs/intro.htm)
- [Write your own Operating System - YouTube Playlist](https://www.youtube.com/watch?v=1rnA6wpF0o4&list=PLHh55M_Kq4OApWScZyPl5HhgsTJS9MZ6M)
- [AptRock327 - OSDev Lecture Series](https://www.youtube.com/watch?v=6n0aYtmgM5Y&list=PL5p37LtXzjqOoEl369i0nlTSaU1O3L-BN)
