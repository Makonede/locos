use crate::{print, println};

const WELCOME: &str = r"___       ________  ________  ________  ________      
|\  \     |\   __  \|\   ____\|\   __  \|\   ____\     
\ \  \    \ \  \|\  \ \  \___|\ \  \|\  \ \  \___|_    
 \ \  \    \ \  \\\  \ \  \    \ \  \\\  \ \_____  \   
  \ \  \____\ \  \\\  \ \  \____\ \  \\\  \|____|\  \  
   \ \_______\ \_______\ \_______\ \_______\____\_\  \ 
    \|_______|\|_______|\|_______|\|_______|\_________\
                                           \|_________|
";

const VERSION: &str = "v0.1.0";

/// Prints the welcome message to the console.
pub fn print_welcome() {
    print!("\x1B[2J");
    println!("{}{}", WELCOME, VERSION);
}
