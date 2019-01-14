//! Yake
//! ``make`` with yaml files.
//!
//! Use yaml files to specify Makefile-like targets and execute these via CLI.
extern crate clap;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
extern crate walkdir;

use args::create_cli_app;
use std::process::exit;
use yaml::{load_yml_from_file, load_yml_from_subdirs};
use std::env;

mod args;
pub mod yake;
mod yaml;

fn main() {
    let yake_args = create_cli_app();

    let yake = load_yml_from_file("Yakefile");

    let path= env::current_dir().unwrap();

    let sub_yakes = match yake.meta.include_recursively {
        Some(true) => load_yml_from_subdirs(path.to_str().unwrap()).unwrap(),
        _ => Vec::new()
    };

    match yake.has_target_name(&yake_args.target) {
        Err(x) => {
            eprintln!(
                "Unknown target: '{}' Available targets are: {:?}",
                yake_args.target, x
            );
            exit(1);
        }
        _ => (),
    };

    yake.execute(&yake_args.target)
        .expect(format!("Execution of target: {} failed.", &yake_args.target).as_str());
}
