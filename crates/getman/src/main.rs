//! The `getman` command. Everything it does lives in the library, which the
//! desktop app shares.

use std::process::ExitCode;

fn main() -> ExitCode {
    getman::main()
}
