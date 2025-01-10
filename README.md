# monitor-affinity

>  Route bars and widgets to monitors based on criteria like "largest" or "rightmost".

`monitor-affinity` supports users who have multi-monitor workspaces and who often change their workspaces, by adding or removing monitors, or connecting monitors to different outputs. `monitor-affinity` routes applications, like desktop bars or widgets, to monitors based on criteria, rather than name. Use it to pin your main bar to the largest monitor, for example, while sending a desktop widget to each secondary screen or run distinct bars based on monitor topology, like "only on my topmost monitor".

`monitor-affinity` only supports X (not Wayland), and requires `libxcb`.

## Installation

### Binary

Linux binary releases are made available on each GitHub release for amd64 and arm64.

### Via Cargo

If you have a recent Rust toolchain installed, you can compile from source via Cargo:

    cargo install monitor-affinity

### Via Nix

`monitor-affinity`'s repo contains a Nix flake.

## Usage

Use `monitor-affinity` with a single command:

    monitor-affinity --affinity largest --affinity leftmost --env MONITOR -- polybar some-bar

or with a config file:

    monitor-affinity --config-file config.toml

where `config.toml` contains

```toml
[[config]]
affinities = [ "largest", "leftmost" ]
env = "MONITOR"
cmd = "polybar"
args = [ "some-bar" ]
```

You can include multiple `[[config]]` stanzas to run multiple commands from a single `monitor-affinity` invocation.

The selected monitor is passed to the invoked command via either an environment variable (`--env` or `env` in config file) or as an command-line argument. The string `%s` will be replaced with the monitor name. For example,

   monitor-affinity -a largest -- my-widget '%s'

might result in executing

    my-widget 'HDMI-0'

To see what commands will be executed, use the `--dry-run` option. See `monitor-affinity --help` for complete options details.

`monitor-affinity` selects monitors by the affinities you specify, in order and to the end. If you request "largest", "leftmost", for example, `monitor-affinity` will select the largest of your monitors (including ties), and then the leftmost (including ties) among that set. This may still yield multiple monitors! The following affinities are supported:

 - primary (important note: X doesn't require you to designate one of the connected monitors as primary, so this affinity may not route to any of your displays)
 - nonprimary
 - largest
 - smallest
 - leftmost
 - rightmost
 - topmost
 - bottommost
 - portrait
 - landscape

Affinities select all matching monitors. "largest", for example, will select the single largest monitor. If multiple monitors are the largest (that is, they are the same size), they are all selected.

All of the affinities may be negated by prefixing them with "not-". "not-largest", for example, will select all of your screens except the largest. If all of your screens are the same size, none will be selected.

If you've set `allow-multiple`, `monitor-affinity` will run the command you've specified once for each monitor in the selected set. Otherwise, it will select the first monitor in the selected set, ordered alphabetically by name.

If your config file includes multiple command configs, they are all evaluated independently. Selection of a monitor for issuing one command has no impact on whether it may be selected for another command.

## License

`monitor-affinity` is (c) 2025 by David Reed and is available under the terms of the MIT License.
