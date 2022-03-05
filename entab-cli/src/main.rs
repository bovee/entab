use std::env::args_os;
use std::io;

use entab_cli::run;

pub fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();

    if let Err(e) = run(args_os(), stdin.lock(), stdout.lock()) {
        eprintln!("##### AN ERROR OCCURRED ####");
        eprintln!("{}", e);
        eprintln!("#####");
    }
}
