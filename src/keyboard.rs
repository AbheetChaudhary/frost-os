use crate::{irq, isr, println};

pub static KBDUS: [u8; 90] = [
   0,  27, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8',	/* 9 */
  b'9', b'0', b'-', b'=', b'\x08',	/* Backspace */
  b'\t',			/* Tab */
  b'q', b'w', b'e', b'r',	/* 19 */
  b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n',	/* Enter key */
    0,			/* 29   - Control */
  b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',	/* 39 */
 b'\'', b'`',   0,		/* Left shift */
 b'\\', b'z', b'x', b'c', b'v', b'b', b'n',			/* 49 */
  b'm', b',', b'.', b'/',   0,				/* Right shift */
  b'*',
    0,	/* Alt */
  b' ',	/* Space bar */
    0,	/* Caps lock */
    0,	/* 59 - F1 key ... > */
    0,   0,   0,   0,   0,   0,   0,   0,
    0,	/* < ... F10 */
    0,	/* 69 - Num lock*/
    0,	/* Scroll Lock */
    0,	/* Home key */
    0,	/* Up Arrow */
    0,	/* Page Up */
  b'-',
    0,	/* Left Arrow */
    0,
    0,	/* Right Arrow */
  b'+',
    0,	/* 79 - End key*/
    0,	/* Down Arrow */
    0,	/* Page Down */
    0,	/* Insert Key */
    0,	/* Delete Key */
    0,   0,   0,
    0,	/* F11 Key */
    0,	/* F12 Key */
    0,	/* All other keys are undefined */
];

pub fn install() {
    irq::IRQRoutines::install_handler(1, keyboard_action);
}

fn keyboard_action(_: &isr::RegState) {
    println!("-- keyboard action --");
}
