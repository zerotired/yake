use std::collections::HashMap;
use std::io;
use std::process::Command;
use std::str;

use colored::Colorize;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::io::Write;

/// Represents the full yaml structure.
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Yake {
    /// Meta data
    pub meta: YakeMeta,
    /// Environment variables
    pub env: Option<HashMap<String, String>>,
    /// Main targets
    pub targets: HashMap<String, YakeTarget>,
    /// Normalized, flattened map of all targets.
    /// Not deserialized from yaml.
    #[serde(skip)]
    all_targets: HashMap<String, YakeTarget>,
    /// Normalized, flattened map of all dependencies.
    /// Not deserialized from yaml.
    #[serde(skip)]
    dependencies: HashMap<String, Vec<YakeTarget>>,
}

/// Contains meta data for the yake object.
///
/// All fields (doc, version) are required. Parsing
/// fails in case values are missing in the yaml data.
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct YakeMeta {
    /// Documentation information
    pub doc: String,
    /// Version information
    pub version: String,
    /// Include Yakefiles of subfolders
    pub include_recursively: Option<bool>,
}

/// Contains meta data for a yake target.
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct YakeTargetMeta {
    /// Documentation information
    pub doc: String,
    /// Type of the target, deserialized from `target`
    #[serde(rename = "type")]
    pub target_type: YakeTargetType,
    /// List of dependent targets
    pub depends: Option<Vec<String>>,
}

/// Defines a yake target. Can have sub-targets.
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct YakeTarget {
    /// Target meta data
    pub meta: YakeTargetMeta,
    /// Subordinate targets
    pub targets: Option<HashMap<String, YakeTarget>>,
    /// List of environment variables
    pub env: Option<HashMap<String, String>>,
    /// List of commands to execute
    /// Will only be executed for `TargetType::Cmd`
    pub exec: Option<Vec<String>>,
}

// Custom deserialization via:
// https://github.com/serde-rs/serde/issues/1019#issuecomment-322966402
/// Defines the different target types.
#[derive(Debug, PartialEq, Clone)]
pub enum YakeTargetType {
    /// A Group has no own commands, just sub-targets.
    Group,
    /// A Callable has no sub-targets, just commands.
    Callable,
}

/// Implements custom serde serializer for the YakeTargetType
impl Serialize for YakeTargetType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match *self {
            YakeTargetType::Group => "group",
            YakeTargetType::Callable => "callable",
        })
    }
}

/// Implements custom serde deserializer for the YakeTargetType
impl<'de> Deserialize<'de> for YakeTargetType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "group" => Ok(YakeTargetType::Group),
            "callable" => Ok(YakeTargetType::Callable),
            _ => Err(D::Error::custom(format!("unknown target type '{}'", s))),
        }
    }
}

/// Implementation for the Yake object
impl Yake {
    /// Get's a list of all existing, callable target names
    pub fn get_target_names(&self) -> Vec<String> {
        self.get_all_targets()
            .iter()
            .filter(|&(_name, target)| target.meta.target_type == YakeTargetType::Callable)
            .map(|(name, _target)| name.clone())
            .collect()
    }

    /// Gets a flattened, normalized map of all target names and it's respective yake
    /// target.
    fn get_all_targets(&self) -> HashMap<String, YakeTarget> {
        let mut targets = HashMap::new();

        for (target_name, target) in &self.targets {
            targets.insert(target_name.to_string(), target.clone());
            if target.targets.is_some() {
                let prefix = target_name.clone();
                targets.extend(target.get_sub_targets(Some(prefix)));
            }
        }

        targets
    }

    /// Checks, whether a specific target name exists.
    pub fn has_target_name(&self, target_name: &str) -> Result<(), Vec<String>> {
        if self.get_target_by_name(target_name).is_some() {
            Ok(())
        } else {
            Err(self.get_target_names().clone())
        }
    }

    /// Gets a YakeTarget by name.
    fn get_target_by_name(&self, target_name: &str) -> Option<YakeTarget> {
        self.get_all_targets()
            .get(&target_name.to_string())
            .cloned()
    }

    /// Gets a normalized, flattened map of all dependencies for each callable target name.
    /// Contains a vector for every callable target in the system, even if a target has no
    /// dependencies.
    fn get_all_dependencies(&self) -> HashMap<String, Vec<YakeTarget>> {
        let mut ret: HashMap<String, Vec<YakeTarget>> = HashMap::new();
        for (target_name, target) in self.get_all_targets() {
            if target.meta.target_type != YakeTargetType::Callable {
                continue;
            }
            ret.insert(target_name.clone(), Vec::new());
            for dependency_name in target.meta.depends.unwrap_or(vec![]).iter() {
                let dep = self.get_target_by_name(dependency_name);
                let dep_target = dep.expect(
                    format!(
                        "Warning: Unknown dependency: {} in target: {}.",
                        dependency_name, target_name
                    )
                    .as_str(),
                );

                ret.get_mut(&target_name).unwrap().push(dep_target);
            }
        }

        ret
    }

    /// Gets a list of dependencies for a target name.
    fn get_dependencies_by_name(&self, target_name: &str) -> Vec<YakeTarget> {
        self.get_all_dependencies()
            .get(target_name)
            .unwrap()
            .clone()
    }

    /// add targets from yakes of subordinate yakes
    pub fn add_sub_yake(&mut self, yake: Yake) -> () {
        yake.get_all_targets().iter().for_each(|(name, target)| {
            &self.targets.insert(name.clone(), target.clone());
            ()
        });
    }

    /// fetches all environment variables of the current target and it's parent targets
    pub fn get_target_env_vars(&self, target_name: &str) -> Result<HashMap<String, String>, String> {
        if self.has_target_name(target_name).is_err() {
            return Err(format!("Unknown target: {}", target_name).to_string());
        }

        let mut envs = self.env.clone().unwrap_or_default();
        let parent_targets: Vec<&str> = target_name.split(".").collect();

        // iterate over parent targets and extend the env with each of them, starting from the
        // highest hierarchy level
        for (i, _t) in parent_targets.iter().enumerate() {
            let parent_target_name = parent_targets[0..i+1].join(".");
            let p = self.get_target_by_name(&parent_target_name).expect(&format!("Unknown Target {}", parent_target_name));
            envs.extend(p.env.unwrap_or_default());
        }

        let target = self.get_target_by_name(target_name).unwrap();
        envs.extend(target.env.unwrap_or_default());

        // filter blacklisted vars like PATH. If not not filtered,
        // the subprocess execution would panic due to path expansion.
        let (invalid, valid): (HashMap<&String, &String>, HashMap<&String, &String>) = envs.iter().partition(|&k| {
            k.0 == "TERM" || k.0 == "TZ" || k.0 == "LANG" || k.0 == "PATH" || k.0 == "HOME"
        });

        if invalid.len() > 0 {
            panic!("{} {:?}", "Found invalid/forbidden env variables".bold().red(), invalid.keys());
        }

        Ok(valid.iter().map(|(&k, &v)| {
            (k.clone(), v.clone())
        }).collect())
    }

    /// Execute a target and it's dependencies.
    pub fn execute(&self, target_name: &str) -> Result<String, String> {
        if self.has_target_name(target_name).is_err() {
            return Err(format!("Unknown target: {}", target_name).to_string());
        }

        let target = self.get_target_by_name(target_name).unwrap();
        let dependencies = self.get_dependencies_by_name(target_name);

        let run_target = |target: &YakeTarget| match target.exec {
            Some(ref commands) => {
                for command in commands {
                    println!(
                        "{} {}:",
                        "↪ Executing".bold().blue(),
                        command.as_str().bold().green()
                    );
                    let output = Command::new("bash")
                        .arg("-c")
                        .arg(command.clone())
                        .envs(self.get_target_env_vars(target_name).unwrap_or_default())
                        .output()
                        .expect(&format!("failed to execute command \"{}\"", command));

                    let stdout_str = str::from_utf8(&output.stdout).unwrap();
                    let stderr_str = str::from_utf8(&output.stderr).unwrap();
                    stdout_str.lines().into_iter().for_each(|line| {
                        io::stdout()
                            .write_all(format!("{}  {}\n", "┆".bold().green(), line).as_bytes())
                            .expect(&format!("failed to write line to stdout \"{}\"", line));
                    });
                    stderr_str.lines().into_iter().for_each(|line| {
                        io::stderr()
                            .write_all(format!("{}  {}\n", "┆".bold().red(), line).as_bytes())
                            .expect(&format!("failed to write line to stderr \"{}\"", line));
                    });
                }
                io::stdout()
                    .write_all(format!("{}\n", "↪ Done".bold().blue()).as_bytes())
                    .expect(&format!("failed to write line to stdout"));
            }
            None => (),
        };

        // run dependencies first
        for dep in dependencies {
            run_target(&dep);
        }

        // then run the actual target
        run_target(&target);

        Ok("All cool".to_string())
    }
}

/// Implementation for a YakeTarget.
impl YakeTarget {
    /// Get a map of subordinate targets.
    pub fn get_sub_targets(&self, prefix: Option<String>) -> HashMap<String, YakeTarget> {
        let mut targets = HashMap::new();
        match self.targets {
            Some(ref x) => {
                for (target_name, target) in x {
                    if target.meta.target_type == YakeTargetType::Callable {
                        let name = match prefix {
                            Some(ref x) => format!("{}.{}", x, target_name),
                            None => target_name.to_string(),
                        };
                        targets.insert(name, target.clone());
                    } else {
                        let p = match prefix {
                            Some(ref x) => Some(format!("{}.{}", x, target_name)),
                            None => None,
                        };
                        targets.extend(target.get_sub_targets(p))
                    }
                }
            }
            None => (),
        }
        targets
    }
}

#[cfg(test)]
mod tests {
    use serde_yaml;

    use super::*;

    fn get_yake_targets() -> HashMap<String, YakeTarget> {
        let mut env = HashMap::new();
        env.insert("WEBAPP_PORT".to_string(), "6543".to_string());
        env.insert("POSTGRES_PORT".to_string(), "5432".to_string());
        let callable_target = YakeTarget {
            targets: None,
            meta: YakeTargetMeta {
                doc: "Huhu".to_string(),
                target_type: YakeTargetType::Callable,
                depends: Some(vec!["base".to_string()]),
            },
            env: Some(env),
            exec: None,
        };

        let mut env_sub = HashMap::new();
        env_sub.insert("BASE".to_string(), "OVERWRITE".to_string());
        env_sub.insert("DOCKER_PORT".to_string(), "1234".to_string());
        env_sub.insert("POSTGRES_PORT".to_string(), "54322".to_string());
        let sub_target = YakeTarget {
            targets: None,
            meta: YakeTargetMeta {
                doc: "Subtarget".to_string(),
                target_type: YakeTargetType::Callable,
                depends: Some(vec!["base".to_string()]),
            },
            env: Some(env_sub),
            exec: None,
        };

        let group_target = YakeTarget {
            targets: Some([("sub".to_string(), sub_target)].iter().cloned().collect()),
            meta: YakeTargetMeta {
                doc: "Grouptarget".to_string(),
                target_type: YakeTargetType::Group,
                depends: None,
            },
            env: None,
            exec: None,
        };

        [
            (
                "base".to_string(),
                YakeTarget {
                    targets: None,
                    meta: YakeTargetMeta {
                        doc: "Base".to_string(),
                        target_type: YakeTargetType::Callable,
                        depends: None,
                    },
                    env: None,
                    exec: None,
                },
            ),
            ("test".to_string(), callable_target),
            ("group".to_string(), group_target),
        ]
        .iter()
        .cloned()
        .collect()
    }

    fn get_yake_dependencies(
        targets: &HashMap<String, YakeTarget>,
    ) -> HashMap<String, Vec<YakeTarget>> {
        let mut dependencies = HashMap::new();
        dependencies.insert(
            "test".to_string(),
            vec![targets.get(&"base".to_string()).unwrap().clone()],
        );
        return dependencies;
    }

    fn get_yake() -> Yake {
        let targets = get_yake_targets();
        let dependencies = get_yake_dependencies(&targets);
        let mut env_root = HashMap::new();
        env_root.insert("BASE".to_string(), "BASEVAL".to_string());

        Yake {
            targets,
            dependencies,
            env: Some(env_root),
            meta: YakeMeta {
                doc: "Bla".to_string(),
                version: "1.0.0".to_string(),
                include_recursively: None,
            },
            all_targets: HashMap::new(),
        }
    }

    #[test]
    fn test_get_all_targets() {
        let yake = get_yake();

        let all_targets = yake.get_all_targets();
        assert_eq!(all_targets.len(), 4);
    }

    #[test]
    fn test_get_all_dependencies() {
        let yake = get_yake();
        let dependencies = yake.get_all_dependencies();
        assert_eq!(dependencies.len(), 3);
        assert_eq!(dependencies.get("test").unwrap().len(), 1);
        assert_eq!(dependencies.get("base").unwrap().len(), 0);
        assert_eq!(dependencies.get("group.sub").unwrap().len(), 1);
    }

    #[test]
    fn test_get_target_by_name() {
        let yake = get_yake();
        assert_eq!(yake.get_target_by_name("group.sub").is_some(), true);
        assert_eq!(yake.get_target_by_name("base").is_some(), true);
        assert_eq!(yake.get_target_by_name("sub").is_none(), true);
    }

    #[test]
    fn test_has_target_name() {
        let yake = get_yake();
        assert_eq!(yake.has_target_name("group.sub").is_ok(), true);
        assert_eq!(yake.has_target_name("sub").is_err(), true);
        assert_eq!(yake.has_target_name("sub").err().unwrap().len(), 3);
    }

    #[test]
    fn test_get_target_names() {
        let yake = get_yake();
        let names = yake.get_target_names();
        assert_eq!(names.len(), 3);
        assert_eq!(names.contains(&"group.sub".to_string()), true);
        assert_eq!(names.contains(&"base".to_string()), true);
        assert_eq!(names.contains(&"test".to_string()), true);
    }

    #[test]
    fn test_get_dependencies_by_name() {
        let yake = get_yake();
        let dependencies = yake.get_dependencies_by_name("group.sub");
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0].meta.doc, "Base".to_string());
    }

    #[test]
    fn test_get_env_vars() {
        let yake = get_yake();

        let envs = yake.get_target_env_vars("base").unwrap_or_default();
        assert_eq!(envs.len(), 1);
        assert_eq!(envs.get("BASE").unwrap(), "BASEVAL");

        let envs = yake.get_target_env_vars("test").unwrap_or_default();
        assert_eq!(envs.len(), 3);
        assert_eq!(envs.get("BASE").unwrap(), "BASEVAL");
        assert_eq!(envs.get("WEBAPP_PORT").unwrap(), "6543");
        assert_eq!(envs.get("POSTGRES_PORT").unwrap(), "5432");

        let envs = yake.get_target_env_vars("group").unwrap_or_default();
        assert_eq!(envs.len(), 1);
        assert_eq!(envs.get("BASE").unwrap(), "BASEVAL");

        let envs = yake.get_target_env_vars("group.sub").unwrap_or_default();
        assert_eq!(envs.len(), 3);
        assert_eq!(envs.get("BASE").unwrap(), "OVERWRITE");
        assert_eq!(envs.get("DOCKER_PORT").unwrap(), "1234");
        assert_eq!(envs.get("POSTGRES_PORT").unwrap(), "54322");
    }

    #[test]
    #[should_panic]
    fn test_get_env_vars_bad() {
        let mut env = HashMap::new();
        env.insert("WEBAPP_PORT".to_string(), "6543".to_string());
        env.insert("PATH".to_string(), "$HOME/bin:$PATH".to_string());
        let mut yake = get_yake();
        yake.env = Some(env);

        yake.get_target_env_vars("base");
    }

    #[test]
    fn test_deserialize_yake_target_type() {
        let yml = r###"
        meta:
          doc: "Some docs"
          version: 1.0.0
        env:
          PATH: $HOME/bin:$PATH
        targets:
          base:
            meta:
              doc: "Test command"
              type: callable
            exec:
              - echo "i'm base"
          group:
            meta:
              doc: "Test command"
              type: group
        "###;

        let yake: Yake = serde_yaml::from_str(&yml).expect("Unable to parse");
        assert_eq!(
            yake.targets.get("base").unwrap().meta.target_type,
            YakeTargetType::Callable
        );
        assert_eq!(
            yake.targets.get("group").unwrap().meta.target_type,
            YakeTargetType::Group
        );
    }

    #[test]
    fn test_add_sub_yakes() {
        let yml = r###"
        meta:
          doc: "Some docs"
          version: 1.0.0
        env:
          PATH: $HOME/bin:$PATH
        targets:
          base:
            meta:
              doc: "Test command"
              type: callable
            exec:
              - echo "i'm base"
          group:
            meta:
              doc: "Test command"
              type: group
        "###;

        let subyml = r###"
        meta:
          doc: "Some docs"
          version: 1.0.0
        env:
          PATH: $HOME/bin:$PATH
        targets:
          base:
            meta:
              doc: "Test command overwritten"
              type: callable
            exec:
              - echo "i'm base, but overwritten by a sub yake"
          sub_base:
            meta:
              doc: "Sub: Test command"
              type: callable
            exec:
              - echo "i'm sub base"
        "###;

        let mut yake: Yake = serde_yaml::from_str(&yml).expect("Unable to parse");
        assert_eq!(
            yake.targets.get("base").unwrap().meta.target_type,
            YakeTargetType::Callable
        );
        assert_eq!(
            yake.targets.get("group").unwrap().meta.target_type,
            YakeTargetType::Group
        );

        let sub_yake: Yake = serde_yaml::from_str(subyml).expect("Unable to parse");
        assert_eq!(
            sub_yake.targets.get("base").unwrap().meta.target_type,
            YakeTargetType::Callable
        );
        assert_eq!(
            sub_yake.targets.get("sub_base").unwrap().meta.target_type,
            YakeTargetType::Callable
        );

        yake.add_sub_yake(sub_yake);
        assert_eq!(
            yake.targets.get("base").unwrap().meta.target_type,
            YakeTargetType::Callable
        );
        assert_eq!(
            yake.targets.get("base").unwrap().meta.doc,
            "Test command overwritten"
        );
        assert_eq!(
            yake.targets.get("sub_base").unwrap().meta.target_type,
            YakeTargetType::Callable
        );
    }
}
