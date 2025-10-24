# swb

Stream Slippi data to SpectatorMode.

## Download

[The latest release can be found here](https://github.com/gcpreston/swb-rs/releases/latest). The [Releases page](https://github.com/gcpreston/swb-rs/releases) has all historical releases and pre-releases.

Make sure to download the correct file for your operating system:

| OS                      | File                                           |
| ----------------------- | ---------------------------------------------- |
| Windows                 | swb-v[version]-x86_64-pc-windows-gnu.zip       |
| macOS (Apple silicon)   | swb-v[version]-aarch64-apple-darwin.tar.gz     |
| macOS (Intel processor) | swb-v[version]-x86_64-apple-darwin.tar.gz      |
| Linux                   | swb-v[version]-x86_64-unknown-linux-gnu.tar.gz |

## Usage

This is a terminal program for the moment; the releases contain helper scripts for convenience.

### Broadcast

- Open Slippi.
- Unzip the downloaded archive and double-click the `broadcast.cmd` or `broadcast.sh` file inside.
- To exit, use Ctrl + C. Make sure the terminal window is focused.

### Spectate

- Unzip the downloaded archive and double-click the `spectate.cmd` or `spectate.sh` file inside.
- Enter the stream ID, found on the SpectatorMode site. If pasting via Ctrl + V doesn't work, right-click and select "Paste".
- To exit, use Ctrl + C. Make sure the terminal window is focused.

### Terminal usage

The basic commands for `swb` itself are:

```bash
# Broadcast to spectatormode.tv
swb broadcast

# Spectate in Dolphin from spectatormode.tv
swb spectate <stream_id>
```

A full list of options can be found using `swb --help` or `swb <command> --help`.

## Troubleshooting

If you are on Mac and get a message like `"broadcast.sh" was not opened` or `"swb" was not opened`:
- Open Settings
- Go to Privacy & Security
- Scroll down to the Security section
- Find `"swb" was blocked to protect your Mac.`, and click the `Open Anyways` button. You will have to confirm, and give your password.

If you are on any platform and the program is blocked by antivirus, either create an exception in the relevant software, or [run the program from source](https://github.com/gcpreston/swb-rs?tab=readme-ov-file#local-development).

## Local development

To run the program from source, simply [install Rust](https://www.rust-lang.org/tools/install), then clone and

```bash
$ cd swb-rs
$ cargo run -- <options>
```
