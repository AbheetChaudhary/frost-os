use std::process::Command;

fn main() {
    // Assembly irq.asm and isr.asm and store them in build/ directory.

    println!("requirements: NASM assembler");
    println!("cargo::rerun-if-changed=src/isr.asm");
    println!("cargo::rerun-if-changed=build/isr.o");

    println!("cargo::rerun-if-changed=src/irq.asm");
    println!("cargo::rerun-if-changed=build/irq.o");

    let _build_isr = Command::new("nasm")
        .args(["-felf32", "src/isr.asm", "-o", "build/isr.o"])
        .output()
        .expect("nasm failed to build isr.asm, make sure `nasm` is installed");

    let _build_irq = Command::new("nasm")
        .args(["-felf32", "src/irq.asm", "-o", "build/irq.o"])
        .output()
        .expect("nasm failed to build irq.asm, make sure `nasm` is installed");
}
