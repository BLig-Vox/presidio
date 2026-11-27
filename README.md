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

### Update the workspace

```sh
presidio --update         # Latest stable build, including switching back from Edge
presidio --update --edge  # Latest published preview build
```

The managed installer adds `~/.local/bin/presidio`; use that full path if the directory isn't on your PATH and you did not install via Binstall. If you installed before these commands were added, rerun the full-workspace installer above once. Updates need Python 3 for release metadata, verify the download checksum, and keep your workspace files. If no Edge build is published, your installation stays unchanged.

### Install with Cargo Binstall (Linux x86-64)

Install [Cargo Binstall](https://github.com/cargo-bins/cargo-binstall#quickly) once if `cargo binstall` is not available. Then install the precompiled Presidio binary from GitHub Releases and set up the full workspace:

```sh
cargo binstall presidio && presidio --desktop
```

The first command downloads the binary without compiling. The second downloads and verifies the Arch/Xfce workspace bundle, then registers Presidio in your applications menu. After setup, run `presidio` or open it from the menu.

### Viewer only (bring your own VNC server)

Use `presidio --viewer-desktop` to register just the viewer, or download and extract the standalone Linux binary from [GitHub Releases](https://github.com/BLig-Vox/presidio/releases/latest).

The viewer shows the 3D desk and can connect to your own VNC server:

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
Run **Build release workspace** from GitHub Actions on `main` with the `edge` channel to publish a prebuilt preview.
