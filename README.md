# Presidio

<img src="assets/presidio.png" width="144" alt="Presidio logo: three blue monitors around a desk">

**A 3D pocket workspace.** Sit at a virtual desk, turn your head across three live monitors, and jump into a screen when it's time to work.

Presidio is a Rust desktop app. On Linux x86-64, the full workspace bundle runs a persistent Arch/Xfce desktop with three 1920 × 1080 displays on one VNC connection. It doesn't require a VM, Docker, or Podman.

[Watch the close-up desk design preview](art/streamer-desk-pov-closer.mp4).

## Quick start

**App only:** Download the Linux x86-64 archive from the [GitHub release](https://github.com/BLig-Vox/presidio/releases/latest). Extract it to a permanent directory, then run `./presidio --desktop`.

If you have [Cargo Binstall](https://github.com/cargo-bins/cargo-binstall), install the prebuilt GitHub release and register the desktop launcher in one line:

```sh
cargo binstall --git https://github.com/BLig-Vox/presidio.git presidio && presidio --desktop
```

Presidio is not on crates.io yet. Once published there, the shorter command will be `cargo binstall presidio && presidio --desktop`.

Open **Presidio** from your applications menu. The standalone app shows the 3D desk and can connect to your own VNC server:

```sh
presidio --vnc 127.0.0.1:5900 --request-triple-layout
```

**Full Arch/Xfce workspace:** Download the workspace archive from the same release and extract it to a permanent directory, then run:

```sh
./presidio --desktop
```

The menu entry launches the bundled desktop when installed from this bundle. Your workspace home persists across launches. The full workspace needs Linux x86-64 and unprivileged user namespaces. See [Workspace setup](WORKSPACE.md) for technical details.

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
