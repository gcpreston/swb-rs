# SpectatorMode Client

Cross-platform GUI for Slippi Web Bridge.

## Download

[The latest release can be found here](https://github.com/gcpreston/swb-rs/releases/latest). The [Releases page](https://github.com/gcpreston/swb-rs/releases) has all historical releases and pre-releases.

Make sure to download the correct file for your operating system:

| OS                      | File                                           |
| ----------------------- | ---------------------------------------------- |
| Windows                 | swb-gui-v[version]-x86_64-pc-windows-gnu.zip       |
| macOS (Apple silicon)   | swb-gui-v[version]-aarch64-apple-darwin.tar.gz     |
| macOS (Intel processor) | swb-gui-v[version]-x86_64-apple-darwin.tar.gz      |
| Linux                   | swb-gui-v[version]-x86_64-unknown-linux-gnu.tar.gz |

## Capabilities

The GUI comes with more limited capabilities than the CLI for the moment. The GUI covers the most basic use cases:
- Broadcasting to spectatormode.tv
- Spectating in Playback Dolphin from spectatormode.tv

It is also possible to target different SpectatorMode hosts. For example, broadcast/spectate against localhost, you can launch the GUI from the terminal with the WebSocket host provided: `swb-gui ws://localhost:4000`

## Troubleshooting

This application is not yet signed, meaning operating systems may not trust it immediately.

If you are on Mac and get a message like `"swb-cli" was not opened`:
- Open Settings
- Go to Privacy & Security
- Scroll down to the Security section
- Find `"swb-cli" was blocked to protect your Mac.`, and click the `Open Anyways` button. You will have to confirm, and give your password.

If you are on Mac and get an error message like `SpectatorMode Client is damaged and can't be opened`:
- Run in Terminal `xattr -c <path/to/SpectatorMode Client.app>`
- This clears any quarantine flags the OS may have set for the unsigned application.

If you are on any platform and the program is blocked by antivirus, either create an exception in the relevant software, or run the program from source.
