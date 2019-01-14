use serde_yaml;
use std::fs::File;
use std::io::prelude::*;
use yake::Yake;
use walkdir::{WalkDir,DirEntry};

pub fn load_yml_from_file(filename: &str) -> Yake {
    let mut f = File::open(filename).expect("File not found.");
    let mut contents = String::new();

    f.read_to_string(&mut contents).expect("Error while reading file.");

    serde_yaml::from_str(&contents).expect("Unable to parse")
}

fn find_yakefiles(directory: &str) -> Result<Vec<DirEntry>, String> {
    let mut files = Vec::new();

    fn is_yakefile_or_dir(entry: &DirEntry) -> bool {
        entry.file_name()
            .to_str()
            .map(|s| s == "Yakefile" || entry.path().is_dir())
            .unwrap_or(false)
    }

    WalkDir::new(directory)
        .min_depth(2)
        .max_depth(2)
        .into_iter()
        .filter_entry(|e| is_yakefile_or_dir(e))
        .filter_map(|v| v.ok())
        .for_each(|v| {
            if v.path().is_file() {
                files.push(v)
            }
        });

    Ok(files)
}

pub fn load_yml_from_subdirs(directory: &str) -> Result<Vec<Yake>, String> {
    let files = find_yakefiles(directory);
    let mut yakes = Vec::new();

    for entry in files.unwrap() {
        yakes.push(load_yml_from_file(entry.path().to_str().unwrap()));
    }

    Ok(yakes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml;

    #[test]
    fn test_find_yakefiles() {
        let dir = ".";

        let files = find_yakefiles(dir);
        assert_eq!(files.unwrap().len(), 1);
    }

    #[test]
    fn test_load_yml_from_subdirs() {
        let dir = ".";

        let sub_yakes = load_yml_from_subdirs(dir);
        assert_eq!(sub_yakes.unwrap().len(), 1);
    }
}