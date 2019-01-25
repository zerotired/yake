use std::collections::HashMap;

use clap::{App, Arg};

#[derive(Debug, PartialEq)]
pub struct YakeArgs {
    pub target: String,
    pub params: HashMap<String, String>,
}

pub fn create_cli_app() -> YakeArgs {
    let matches = App::new("Yake")
        .version("0.1")
        .author("Tim Eggert <tim@elbart.com>")
        .about("Make with yaml files")
        .arg(Arg::with_name("TARGET")
            .help("Target to invoke")
            .required(true)
            .index(1))
        .arg(Arg::with_name("param")
            .help("Parameters for the yake processing")
            .takes_value(true)
            .short("p")
            .long("parameter")
            .multiple(true)
            .required(false)
            .requires("TARGET"))
        .get_matches();

    let target = matches.value_of("TARGET").expect("No target specified").trim();

    let mut args = YakeArgs { target: target.to_string(), params: HashMap::new() };

    if let Some(parameter_values) = matches.values_of("param") {
        for param in parameter_values {
            match param.trim().split("=").collect::<Vec<&str>>().as_slice() {
                [first, last] => args.params.insert(first.to_string(), last.to_string()),
                _ => None
            };
        }
    }

    args
}