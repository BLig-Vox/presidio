# Presidio: stabilize input capture and the 3D desk

Status: Implemented locally. Hyprland verification remains open.

- [x] **First capture:** Clicking “Yes, capture” keeps the current 3D desk view. Ordinary typing and clicks go to the guest. `Super + Enter` explicitly toggles flat monitor view without releasing capture; `Super + Esc` returns input to the host.
- [x] **Steady head turns:** `Super + right-drag` turns left or right without pitching up or down, even if the mouse drifts vertically. The gesture must not send a right-click to VNC.
- [x] **Zoom:** `Super + mouse wheel` and `Super + +` / `Super + -` zoom the 3D desk without switching views, releasing capture, or scrolling the guest.
- [x] **Full HD:** Verify all three VNC displays are 1920×1080 and keep their native-resolution feeds. The 3D view can show all three at once; flat monitor view makes one fill the window for sharper text.
- [x] **Held keys:** Preserve Shift, Ctrl, and Alt until physical release. Repeat held Backspace and arrow keys in the guest without duplicate repeats or repeating modifiers. Keep Super local. Clear held keys and mouse buttons on capture loss, release, or disconnect.
- [x] **Responsiveness:** Keep VNC writes off the window thread so the camera and `Super + Esc` work even when the guest stops reading. Fix a reproduced Wayland freeze when releasing a key cancels its repeat timer before the window thread can read it.

## Verification

- [x] X11 with TigerVNC: three 1920×1080 displays, first capture in 3D, camera look and zoom, flat view, shifted text, Ctrl/Alt chords, and held Backspace/Right. Switching back to the desk and releasing capture both worked while the guest process was stopped.
- [x] Native Wayland on Sway: shortcut inhibition, pointer confinement, first capture in 3D, local camera controls, held Shift and Backspace, release of held inputs, and cleanup on compositor focus loss or guest disconnect.
- [x] Regression test: a socket that stops reading cannot block the input queue or keep a failed VNC session alive.
- [ ] Hyprland: the nested compositor failed to start (`CBackend::create() failed!`); direct capture on Hyprland remains unverified.
