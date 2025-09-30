use crate::{print, ps2::keyboard::{KeyEvent, KEYBOARD}};
use x86_64::instructions::interrupts;

/// consumes input from the keyboard buffer
pub fn locos_shell() -> ! {
    loop {
        let (event, state) = interrupts::without_interrupts(|| {
            let mut keyboard_lock = KEYBOARD.lock();
            if let Some(ref mut keyboard) = *keyboard_lock {
                let event = keyboard.read_key();
                let state = keyboard.get_state();
                (event, state)
            } else {
                (None, Default::default())
            }
        });

        if let Some(KeyEvent::KeyDown(scancode)) = event
            && let Some(character) = scancode.to_char(state.shift_pressed(), state.caps_lock) {
                print!("{}", character);
            } else {
                core::hint::spin_loop();
            }
    }
}
