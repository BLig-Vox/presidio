# Presidio

<img src="assets/presidio.png" width="144" alt="Presidio logo: three blue monitors around a desk">

**A 3D pocket workspace.** Sit at a virtual desk, turn your head across three live monitors, and jump into a screen when it's time to work.

Presidio is a Rust desktop app. On Linux x86-64, the full workspace bundle runs a persistent Arch/Xfce desktop with three 1920 × 1080 displays on one VNC connection. It doesn't require a VM, Docker, or Podman.

[Watch the close-up desk design preview](art/streamer-desk-pov-closer.mp4).

## Quick start

**Full workspace (Linux x86-64):** Install Presidio with its bundled Arch/Xfce desktop and three 1080p displays. No separate VNC server, Docker, Podman, or Cargo needed.

```sh
installer=$(curl -fsSL https://raw.githubusercontent.com/BLig-Vox/presidio/main/scripts/install-workspace) && bash -c "$installer"
```

Open **Presidio** from your applications menu. The workspace starts automatically and keeps your files between launches. The host must allow unprivileged user namespaces. See [Workspace setup](WORKSPACE.md) for details.

### Viewer only (bring your own VNC server)

For a manual install, Download the Linux x86-64 archive from the [GitHub release](https://github.com/BLig-Vox/presidio/releases/latest). Extract it to a permanent directory, then run `./presidio --desktop`.

**With Cargo Binstall:** If `cargo binstall` is not available, install the subcommand once:

```sh
cargo install cargo-binstall --locked
cargo binstall --version
```

If Cargo still reports `no such command: binstall`, add Cargo’s bin directory to your `PATH` with `export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"` and retry. To install Binstall itself without compiling, see the [official prebuilt installer](https://github.com/cargo-bins/cargo-binstall#quickly).

Then install the prebuilt Presidio release and register the desktop launcher in one line:

```sh
cargo binstall --git https://github.com/BLig-Vox/presidio.git presidio && presidio --desktop
```

Presidio is not on crates.io yet. Once published there, the shorter command will be `cargo binstall presidio && presidio --desktop`.

Open **Presidio** from your applications menu. The standalone app shows the 3D desk and can connect to your own VNC server:

```sh
presidio --vnc 127.0.0.1:5900 --request-triple-layout
```

## Controls

| Control | Action |
| --- | --- |
| Super + right-drag | Look around the desk |
| Super + Enter | Focus a monitor or release input back to the desk |
| Home | Recenter your view |

## Build from source

```sh
cargo run --locked
```

Maintainers can run `scripts/package-release` for the binary archive or `scripts/package-release --workspace` to also package a prepared workspace runtime.
