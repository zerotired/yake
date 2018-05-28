use serde_yaml;
use std::fs::File;
use std::io::prelude::*;
use yake::Yake;

pub fn load_yml_from_file(filename: &str) -> Yake {
    let mut f = File::open(filename).expect("File not found.");
    let mut contents = String::new();

    f.read_to_string(&mut contents).expect("Error while reading file.");

    serde_yaml::from_str(&contents).expect("Unable to parse")
}