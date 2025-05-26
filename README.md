# swb

Stream Slippi data to ssbm.tv.

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

- Open Slippi.
- Unzip the downloaded archive and run the `swb.exe` or `swb` file inside.
- To exit, use Ctrl + C. Make sure the terminal window is focused.
- For additional options, run with the `--help` flag (via command line): `swb --help`.

## Troubleshooting

If you are on Mac and get a message like `"swb" was not opened`:
- Open Settings
- Go to Privacy & Security
- Scroll down to the Security section
- Find `"swb" was blocked to protect your Mac.`, and click the `Open Anyways` button. You will have to confirm, and give your password.
