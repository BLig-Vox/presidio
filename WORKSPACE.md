# Presidio Linux workspace

On Linux x86-64, run `scripts/arch-oci --launch` from a Presidio package. This starts the bundled Arch/Xfce image and connects the native client over a private Unix VNC socket. It does not use Docker, Podman, a VM, or a system daemon. The client requests three 1920×1080 displays.

The workspace home is persisted under `${XDG_DATA_HOME:-$HOME/.local/share}/presidio/arch-workspace/home`. Existing workspace homes are reused in place. Set `MRP_WORKSPACE_STATE` to use a separate workspace. The image is checked against the shipped SHA-256 file on first use and cached locally. Xfce defaults to Adwaita Dark, Papirus Dark, and a custom mountain wallpaper; subsequent customization is stored in the workspace home.

The host needs unprivileged Linux user namespaces. The nested desktop used for development blocks a private procfs mount; on this trusted workspace host only, launch with `MRP_NESTED_PROC=1` to use the outer procfs read-only and disable Glycin’s secondary image-loader sandbox, which cannot use the mismatched PID view. The normal Linux launcher does not use this workaround. This is a curated desktop workspace, not a general-purpose isolation boundary for untrusted containers.

To assemble the package, include the client binary, its assets, and `art/workspace/` plus `scripts/arch-oci` and `runtime/linux-x86_64/` (crun, umoci, `arch-xfce.oci.tar`, and `arch-xfce.sha256`). `scripts/pack-workspace-image` packs an Arch root filesystem prepared with the desktop and VNC packages. Instead of shipping the archive, a release can ship a matching `arch-xfce.url` pointing to an HTTPS download and the same digest.
