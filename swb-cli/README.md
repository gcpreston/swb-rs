# swb-cli

The command-line interface for Slippi Web Bridge.

## Download

[The latest release can be found here](https://github.com/gcpreston/swb-rs/releases/latest). The [Releases page](https://github.com/gcpreston/swb-rs/releases) has all historical releases and pre-releases.

Make sure to download the correct file for your operating system:

| OS                      | File                                           |
| ----------------------- | ---------------------------------------------- |
| Windows                 | swb-cli-v[version]-x86_64-pc-windows-gnu.zip       |
| macOS (Apple silicon)   | swb-cli-v[version]-aarch64-apple-darwin.tar.gz     |
| macOS (Intel processor) | swb-cli-v[version]-x86_64-apple-darwin.tar.gz      |
| Linux                   | swb-cli-v[version]-x86_64-unknown-linux-gnu.tar.gz |

## Usage

```bash
# Broadcast to spectatormode.tv
swb-cli broadcast

# Spectate in Dolphin from spectatormode.tv
swb-cli spectate <stream_id>
```

A full list of options can be found using `swb-cli --help` or `swb-cli <command> --help`.

## Troubleshooting

If you are on Mac and get a message like `"swb-cli" was not opened`:
- Open Settings
- Go to Privacy & Security
- Scroll down to the Security section
- Find `"swb-cli" was blocked to protect your Mac.`, and click the `Open Anyways` button. You will have to confirm, and give your password.

If you are on any platform and the program is blocked by antivirus, either create an exception in the relevant software, or [run the program from source](https://github.com/gcpreston/swb-rs?tab=readme-ov-file#local-development).

## Local development

To run the program from source, simply [install Rust](https://www.rust-lang.org/tools/install), then clone and

```bash
$ cd swb-rs
$ cargo run -bin swb-cli -- <options>
```
