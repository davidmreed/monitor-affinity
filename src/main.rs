use serde::Deserialize;
use xcb::{self, randr, randr::MonitorInfo, x};

#[derive(Copy, Clone, Debug, Deserialize)]
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
    HiDPI,
}

#[derive(Debug, Deserialize)]
struct Config {
    cmd: String,
    args: Option<Vec<String>>,
    affinities: Vec<Affinity>,
    allow_multiple: Option<bool>,
    env: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    dry_run: Option<bool>,
    config: Vec<Config>,
}

#[derive(Clone, Debug)]
struct Monitor {
    width: u32,
    height: u32,
    x: i16,
    y: i16,
    primary: bool,
    width_mm: u32,
    height_mm: u32,
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
        // The name is Latin-1 encoded.
        let as_str = reply.name().as_bytes().iter().map(|&c| c as char).collect();

        Ok(Monitor {
            x: m.x(),
            y: m.y(),
            width: m.width().into(),
            height: m.height().into(),
            name: as_str,
            primary: m.primary(),
            width_mm: m.width_in_millimeters(),
            height_mm: m.height_in_millimeters(),
        })
    }
}

fn get_monitors_for_affinities<'a>(
    affinities: &Vec<Affinity>,
    monitors: &Vec<Monitor>,
) -> Vec<Monitor> {
    let mut monitors = monitors.clone();

    for affinity in affinities.iter() {
        match affinity {
            Affinity::Primary
            | Affinity::Nonprimary
            | Affinity::Portrait
            | Affinity::Landscape
            | Affinity::HiDPI => {
                let key_func = match affinity {
                    Affinity::Primary => |a: &Monitor| a.primary,
                    Affinity::Nonprimary => |a: &Monitor| !a.primary,
                    Affinity::Portrait => |a: &Monitor| a.x > a.y,
                    Affinity::Landscape => |a: &Monitor| a.y > a.x,
                    Affinity::HiDPI => |a: &Monitor| (a.width / (a.width_mm / 2.54)) > 72,
                    _ => |_: &Monitor| false,
                };
                monitors.retain(|m| key_func(m));
            }
            Affinity::Largest
            | Affinity::Smallest
            | Affinity::Leftmost
            | Affinity::Rightmost
            | Affinity::Topmost
            | Affinity::Bottommost => {
                let key_func = match affinity {
                    Affinity::Largest => |a: &Monitor| (a.width * a.height) as i64,
                    Affinity::Smallest => |a: &Monitor| -((a.width * a.height) as i64),
                    Affinity::Leftmost => |a: &Monitor| -(a.x as i64),
                    Affinity::Rightmost => |a: &Monitor| a.x as i64,
                    Affinity::Topmost => |a: &Monitor| -(a.y as i64),
                    Affinity::Bottommost => |a: &Monitor| a.y as i64,
                    _ => |_: &Monitor| 0i64,
                };
                monitors.sort_unstable_by_key(key_func);

                if monitors.len() > 1 {
                    let first = key_func(&monitors[0]);
                    monitors.retain(|a| key_func(a) == first);
                }
            }
        }
    }

    // Ensure we have a deterministic order if we don't have enough affinities
    // to select a single monitor.
    monitors.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    monitors
}

fn main() -> Result<(), anyhow::Error> {
    let (conn, screen_num) = xcb::Connection::connect(None)?;

    // Fetch the `x::Setup` and get the main `x::Screen` object.
    let setup = conn.get_setup();
    let screen = setup.roots().nth(screen_num as usize).unwrap();
    let window: x::Window = conn.generate_id();
    let cookie = conn.send_request_checked(&x::CreateWindow {
        depth: x::COPY_FROM_PARENT as u8,
        wid: window,
        parent: screen.root(),
        x: 0,
        y: 0,
        width: 150,
        height: 150,
        border_width: 0,
        class: x::WindowClass::InputOutput,
        visual: screen.root_visual(),
        // this list must be in same order than `Cw` enum order
        value_list: &[
            x::Cw::BackPixel(screen.white_pixel()),
            x::Cw::EventMask(x::EventMask::EXPOSURE | x::EventMask::KEY_PRESS),
        ],
    });
    conn.check_request(cookie)?;
    let cookie = conn.send_request(&randr::GetMonitors {
        window,
        get_active: false,
    });
    let monitor_reply: randr::GetMonitorsReply = conn.wait_for_reply(cookie)?;
    let config_file: ConfigFile = toml::from_str(
        r#"
    dry_run = true
    [[config]]
    cmd = "polybar"
    args = [ "main-bar" ]
    env = "MONITOR"
    affinities = [ "Primary" ]
    [[config]]
    cmd = "polybar"
    env = "MONITOR"
    args = [ "second-bar" ]
    affinities = [ "Nonprimary" ]
    allow_multiple = true
    "#,
    )?;
    let config = config_file.config;
    let monitors: Result<Vec<Monitor>, anyhow::Error> = monitor_reply
        .monitors().map(|m| m.try_into()).collect();
    let monitors = monitors?;

    for c in config.iter() {
        let monitors = get_monitors_for_affinities(&c.affinities, &monitors);
        if monitors.len() > 0 {
            let max = if c.allow_multiple.unwrap_or(false) {
                monitors.len()
            } else {
                1
            };

            for i in 0..max {
                let monitor = &monitors[i];
                let mut cmd = std::process::Command::new(&c.cmd);
                if let Some(args) = &c.args {
                    cmd.args(args.into_iter().map(|s| s.replace("%s", &monitor.name)));
                }
                if let Some(env) = &c.env {
                    cmd.env(env, &monitor.name);
                }
                if config_file.dry_run.unwrap_or(false) {
                    println!("{:?}", cmd);
                } else {
                    cmd.spawn()?;
                }
            }
        }
    }

    Ok(())
}
