use clap::{Args, Parser};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use xcb::{self, randr, randr::MonitorInfo, x};

#[derive(Copy, Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Affinity {
    Primary,
    Nonprimary,
    Largest,
    Smallest,
    Leftmost,
    Rightmost,
    Topmost,
    Bottommost,
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, PartialEq)]
struct AffinityPair(Affinity, bool);

impl std::str::FromStr for AffinityPair {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split("-").collect();
        if parts.len() == 1 {
            Ok(AffinityPair(
                toml::Value::String(parts[0].into()).try_into()?,
                true,
            ))
        } else if parts[0] != "not" {
            Err(anyhow::Error::msg("invalid affinity"))
        } else {
            Ok(AffinityPair(
                toml::Value::String(parts[1].into()).try_into()?,
                false,
            ))
        }
    }
}

impl<'de> serde::Deserialize<'de> for AffinityPair {
    fn deserialize<D>(deserializer: D) -> Result<AffinityPair, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize, Args)]
struct Config {
    /// The command to execute with monitor affinity.
    cmd: String,
    /// Arguments to pass to the command. %s will be replaced with the name of the preferred
    /// monitor.
    args: Option<Vec<String>>,
    /// One or more monitor affinities, evaluated in order to select preferred monitor.
    #[arg(short, long = "affinity", required = true)]
    affinities: Vec<AffinityPair>,
    /// If true, and multiple monitors match the given affinities, run the command once per
    /// monitor.
    #[arg(short = 'm', long, default_value_t = false)]
    #[serde(default)]
    allow_multiple: bool,
    /// Set an env var to the name of the preferred monitor.
    #[arg(short, long)]
    env: Option<String>,
}

impl Config {
    fn get_commands_for_monitors(&self, monitors: &[Monitor]) -> Vec<std::process::Command> {
        let monitors = get_monitors_for_affinities(&self.affinities, monitors);
        let mut ret = Vec::new();
        if !monitors.is_empty() {
            let max = if self.allow_multiple {
                monitors.len()
            } else {
                1
            };

            for monitor in monitors.iter().take(max) {
                let mut cmd = std::process::Command::new(&self.cmd);
                if let Some(args) = &self.args {
                    cmd.args(args.iter().map(|s| s.replace("%s", &monitor.name)));
                }
                if let Some(env) = &self.env {
                    cmd.env(env, &monitor.name);
                }
                ret.push(cmd);
            }
        }

        ret
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct CliConfig {
    /// Print what commands would be run, but don't run them.
    #[arg(short, long, default_value_t = false)]
    dry_run: bool,
    #[command(flatten)]
    cli_config: Option<Config>,
    /// Read configuration from a TOML file. Required for running more than one command.
    #[arg(long, conflicts_with_all=["cmd", "args", "env", "affinities", "allow_multiple"])]
    config_file: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    config: Vec<Config>,
}

#[derive(Clone, Debug)]
struct Monitor {
    width: u32,
    height: u32,
    x: i16,
    y: i16,
    primary: bool,
    name: String,
}

impl TryFrom<&MonitorInfo> for Monitor {
    type Error = anyhow::Error;

    fn try_from(m: &MonitorInfo) -> Result<Self, Self::Error> {
        let (conn, _) = xcb::Connection::connect(None)?;
        let cookie = conn.send_request(&x::GetAtomName {
            atom: m.name().to_owned(),
        });
        let reply: x::GetAtomNameReply = conn.wait_for_reply(cookie)?;
        // The name is Latin-1 encoded. Latin-1 codepoints are UTF-8 compatible,
        // but Latin-1 encoding is not.
        let as_str = reply.name().as_bytes().iter().map(|&c| c as char).collect();

        Ok(Monitor {
            x: m.x(),
            y: m.y(),
            width: m.width().into(),
            height: m.height().into(),
            name: as_str,
            primary: m.primary(),
        })
    }
}

fn get_monitors_for_affinities(affinities: &[AffinityPair], monitors: &[Monitor]) -> Vec<Monitor> {
    let mut monitors = monitors.to_owned();

    for AffinityPair(affinity, inclusive) in affinities.iter() {
        match affinity {
            Affinity::Primary | Affinity::Nonprimary | Affinity::Portrait | Affinity::Landscape => {
                let key_func = match affinity {
                    Affinity::Primary => |a: &Monitor| a.primary,
                    Affinity::Nonprimary => |a: &Monitor| !a.primary,
                    Affinity::Landscape => |a: &Monitor| a.width > a.height,
                    Affinity::Portrait => |a: &Monitor| a.height > a.width,
                    _ => |_: &Monitor| false,
                };
                monitors.retain(|m| {
                    if *inclusive {
                        key_func(m)
                    } else {
                        !key_func(m)
                    }
                });
            }
            Affinity::Largest
            | Affinity::Smallest
            | Affinity::Leftmost
            | Affinity::Rightmost
            | Affinity::Topmost
            | Affinity::Bottommost => {
                let key_func = match affinity {
                    Affinity::Largest => |a: &Monitor| -((a.width * a.height) as i64),
                    Affinity::Smallest => |a: &Monitor| ((a.width * a.height) as i64),
                    Affinity::Rightmost => |a: &Monitor| -(a.x as i64),
                    Affinity::Leftmost => |a: &Monitor| a.x as i64,
                    Affinity::Topmost => |a: &Monitor| -(a.y as i64),
                    Affinity::Bottommost => |a: &Monitor| a.y as i64,
                    _ => |_: &Monitor| 0i64,
                };
                monitors.sort_unstable_by_key(key_func);

                if monitors.len() > 1 {
                    let first = key_func(&monitors[0]);
                    monitors.retain(|a| {
                        if *inclusive {
                            key_func(a) == first
                        } else {
                            key_func(a) != first
                        }
                    });
                } else if !*inclusive {
                    // If this is a "not-X" affinity and we have exactly one monitor,
                    // we match none. No monitor is "not largest" when we have one monitor.
                    monitors.retain(|_| false);
                }
            }
        }
    }

    // Ensure we have a deterministic order if we don't have enough affinities
    // to select a single monitor.
    monitors.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    monitors
}

fn get_connection() -> Result<(xcb::Connection, x::Window), anyhow::Error> {
    let (conn, screen_num) = xcb::Connection::connect(None)?;

    // TODO: use conn.active_extensions() to check for randr https://docs.rs/xcb/latest/xcb/struct.Connection.html#method.active_extensions

    let setup = conn.get_setup();
    let screen = setup.roots().nth(screen_num as usize).unwrap();
    let window: x::Window = conn.generate_id();
    let cookie = conn.send_request_checked(&x::CreateWindow {
        depth: x::COPY_FROM_PARENT as u8,
        wid: window,
        parent: screen.root(),
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        border_width: 0,
        class: x::WindowClass::InputOutput,
        visual: screen.root_visual(),
        value_list: &[x::Cw::BackPixel(screen.white_pixel())],
    });
    conn.check_request(cookie)?;

    Ok((conn, window))
}

fn get_monitors() -> Result<Vec<Monitor>, anyhow::Error> {
    let (conn, window) = get_connection()?;
    let cookie = conn.send_request(&randr::GetMonitors {
        window,
        get_active: false,
    });
    let monitor_reply: randr::GetMonitorsReply = conn.wait_for_reply(cookie)?;
    let monitors: Result<Vec<Monitor>, anyhow::Error> =
        monitor_reply.monitors().map(|m| m.try_into()).collect();

    monitors
}

fn main() -> Result<(), anyhow::Error> {
    let conf = CliConfig::parse();
    let mut configs = vec![];

    if let Some(cli_config) = conf.cli_config {
        configs.push(cli_config);
    }

    if let Some(path) = conf.config_file {
        let config_file: ConfigFile = toml::from_str(&fs::read_to_string(path)?)?;
        configs.extend(config_file.config);
    }

    let monitors = get_monitors()?;

    for c in configs.iter() {
        let commands = c.get_commands_for_monitors(&monitors);
        for mut cmd in commands.into_iter() {
            if conf.dry_run {
                println!("{:?}", cmd);
            } else {
                cmd.spawn()?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn primary() -> Monitor {
        Monitor {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            primary: true,
            name: "PRIMARY".into(),
        }
    }

    fn large() -> Monitor {
        Monitor {
            x: 1920,
            y: 0,
            width: 3440,
            height: 1440,
            primary: false,
            name: "LARGE".into(),
        }
    }
    fn top() -> Monitor {
        Monitor {
            x: 0,
            y: 1440,
            width: 1024,
            height: 768,
            primary: false,
            name: "TOP".into(),
        }
    }

    fn portrait() -> Monitor {
        Monitor {
            x: 0,
            y: 1080,
            width: 768,
            height: 1024,
            primary: false,
            name: "PORTRAIT".into(),
        }
    }

    #[test]
    fn test_affinities_largest() {
        let monitors = vec![primary(), large()];
        let affinities = vec![AffinityPair(Affinity::Largest, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("LARGE", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_smallest() {
        let monitors = vec![large(), primary()];
        let affinities = vec![AffinityPair(Affinity::Smallest, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("PRIMARY", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_primary() {
        let monitors = vec![large(), primary()];
        let affinities = vec![AffinityPair(Affinity::Primary, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("PRIMARY", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_nonprimary() {
        let monitors = vec![primary(), large()];
        let affinities = vec![AffinityPair(Affinity::Nonprimary, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("LARGE", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_leftmost() {
        let monitors = vec![primary(), large()];
        let affinities = vec![AffinityPair(Affinity::Leftmost, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("PRIMARY", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_rightmost() {
        let monitors = vec![primary(), large()];
        let affinities = vec![AffinityPair(Affinity::Rightmost, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("LARGE", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_topmost() {
        let monitors = vec![primary(), top()];
        let affinities = vec![AffinityPair(Affinity::Topmost, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("TOP", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_bottommost() {
        let monitors = vec![top(), primary()];
        let affinities = vec![AffinityPair(Affinity::Bottommost, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("PRIMARY", selected_monitors[0].name);
    }

    #[test]
    fn test_negative_affinities() {
        let monitors = vec![top(), primary()];
        let affinities = vec![AffinityPair(Affinity::Bottommost, false)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("TOP", selected_monitors[0].name);
    }

    #[test]
    fn test_negative_affinities_xmost_single() {
        let monitors = vec![top()];
        let affinities = vec![AffinityPair(Affinity::Bottommost, false)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(0, selected_monitors.len());
    }

    #[test]
    fn test_affinities_landscape() {
        let monitors = vec![portrait(), primary()];
        let affinities = vec![AffinityPair(Affinity::Landscape, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("PRIMARY", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_portrait() {
        let monitors = vec![primary(), portrait()];
        let affinities = vec![AffinityPair(Affinity::Portrait, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("PORTRAIT", selected_monitors[0].name);
    }

    #[test]
    fn test_affinities_matches_all() {
        let monitors = vec![primary(), top(), large()];
        let affinities = vec![AffinityPair(Affinity::Landscape, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(3, selected_monitors.len());
    }

    #[test]
    fn test_affinities_matches_none() {
        let monitors = vec![primary(), top(), large()];
        let affinities = vec![AffinityPair(Affinity::Portrait, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(0, selected_monitors.len());
    }

    #[test]
    fn test_affinities_matches_multiple_but_not_all() {
        let monitors = vec![primary(), top(), large(), portrait()];
        let affinities = vec![AffinityPair(Affinity::Landscape, true)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(3, selected_monitors.len());
        assert!(selected_monitors.iter().is_sorted_by_key(|m| &m.name));
    }

    #[test]
    fn test_affinities_matches_exclusive_sort_based() {
        let monitors = vec![primary(), top(), large(), portrait()];
        let affinities = vec![AffinityPair(Affinity::Largest, false)];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(3, selected_monitors.len());
        assert_eq!("PORTRAIT", selected_monitors[0].name);
        assert_eq!("PRIMARY", selected_monitors[1].name);
        assert_eq!("TOP", selected_monitors[2].name);
    }

    #[test]
    fn test_affinities_matches_exclusive_binary() {
        let monitors = vec![primary(), top(), large(), portrait()];
        let affinities = vec![
            AffinityPair(Affinity::Landscape, false),
            AffinityPair(Affinity::Leftmost, true),
        ];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("PORTRAIT", selected_monitors[0].name);
    }

    #[test]
    fn test_affinity_pair_parsing() {
        assert_eq!(
            AffinityPair(Affinity::Largest, true),
            "largest".parse().unwrap()
        );
        assert_eq!(
            AffinityPair(Affinity::Largest, false),
            "not-largest".parse().unwrap()
        );
    }

    #[test]
    fn test_affinities_matches_multiple_criteria() {
        let monitors = vec![primary(), top(), large(), portrait()];
        let affinities = vec![
            AffinityPair(Affinity::Landscape, true),
            AffinityPair(Affinity::Leftmost, true),
            AffinityPair(Affinity::Bottommost, true),
        ];
        let selected_monitors = get_monitors_for_affinities(&affinities, &monitors);
        assert_eq!(1, selected_monitors.len());
        assert_eq!("PRIMARY", selected_monitors[0].name);
    }

    #[test]
    fn test_get_commands_for_monitors_single() {
        let config = Config {
            cmd: "foobar".into(),
            args: Some(vec!["baz".into()]),
            affinities: vec![AffinityPair(Affinity::Largest, true)],
            allow_multiple: false,
            env: Some("MONITOR".into()),
        };
        let commands = config.get_commands_for_monitors(&[primary(), large()]);
        assert_eq!(1, commands.len());
        assert_eq!(
            format!("{:?}", commands[0]),
            r#"MONITOR="LARGE" "foobar" "baz""#
        );
    }

    #[test]
    fn test_get_commands_for_monitors_multiple() {
        let config = Config {
            cmd: "foobar".into(),
            args: Some(vec!["baz".into()]),
            affinities: vec![AffinityPair(Affinity::Nonprimary, true)],
            allow_multiple: true,
            env: Some("MONITOR".into()),
        };
        let commands = config.get_commands_for_monitors(&[top(), large()]);
        assert_eq!(2, commands.len());
        assert_eq!(
            format!("{:?}", commands[0]),
            r#"MONITOR="LARGE" "foobar" "baz""#
        );
        assert_eq!(
            format!("{:?}", commands[1]),
            r#"MONITOR="TOP" "foobar" "baz""#
        );
    }
}
