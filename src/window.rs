//! The window outlives the process: it reopens where it was left, and a second launch brings it forward instead of
//! starting another player.
use fastframe_instance::{Claim, Slot};
use fastframe_shell::window::Geometry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

const APP_ID: &str = "com.radio4000.Radio4000";

/// The size, place and maximized state, as `window.json` keeps them.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    size: Option<[f32; 2]>,
    position: Option<[f32; 2]>,
    maximized: bool,
}

/// Where the window is remembered, and the writer that saves it off the drawing thread.
pub struct Memory {
    path: Option<PathBuf>,
    pub geometry: Geometry,
    files: fastframe_file::Writer,
    /// Hyprland marks every window maximized, floating or not, so there it is asked instead.
    hyprland: bool,
    /// The size last looked at under Hyprland, and whether the window floated then.
    seen: Option<[f32; 2]>,
    floating: Option<bool>,
}

impl Memory {
    /// The demo neither reads nor saves, so its screenshots open the same every time.
    pub fn load(demo: bool) -> Self {
        let path = (!demo)
            .then(|| fastframe_dirs::Dirs::platform("com", "radio4000", "Radio4000"))
            .flatten()
            .map(|dirs| dirs.config.join("window.json"));
        let hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();
        let mut geometry = path.as_deref().map(read).unwrap_or_default();
        geometry.maximized &= !hyprland;
        Self {
            path,
            geometry,
            files: fastframe_file::Writer::new(),
            hyprland,
            seen: None,
            floating: None,
        }
    }

    /// Call every frame: saves when the window was moved, resized or maximized.
    pub fn remember(&mut self, ctx: &egui::Context) {
        let changed = if self.hyprland {
            self.follow_hyprland(ctx)
        } else {
            self.geometry.remember(ctx)
        };
        if changed && let Some(path) = &self.path {
            self.files.write(
                path,
                encode(&self.geometry),
                fastframe_file::Access::Default,
            );
        }
    }
}

impl Memory {
    /// Hyprland tiles a new window at the tile's size whatever it asked for, so only a floating window's size is
    /// kept, and floating the window again brings that size back. Returns whether the size changed.
    fn follow_hyprland(&mut self, ctx: &egui::Context) -> bool {
        let zoom = ctx.zoom_factor();
        let size = ctx.input(|i| {
            let v = i.viewport();
            let normal = v.minimized != Some(true) && v.fullscreen != Some(true);
            i.raw
                .screen_rect
                .filter(|_| normal)
                .map(|r| (r.size() * zoom).into())
        });
        let Some(size) = size.filter(|&size| self.seen.is_none_or(|seen| differs(seen, size)))
        else {
            return false;
        };
        // Not mapped yet: ask again next frame.
        let Some(floating) = hyprland::floating() else {
            return false;
        };
        self.seen = Some(size);
        let was = self.floating.replace(floating);
        match step(self.geometry.size, size, floating, was) {
            Step::Keep => false,
            Step::Save => {
                self.geometry.size = Some(size);
                true
            }
            Step::Restore(size) => {
                hyprland::resize(size);
                false
            }
        }
    }
}

/// What a new size under Hyprland means for the saved one.
#[derive(Debug, PartialEq)]
enum Step {
    /// Tiled, or the same: the tile's size is the layout's, not the user's.
    Keep,
    Save,
    /// It has just started floating, at the compositor's size: put it back at the user's.
    Restore([f32; 2]),
}

fn step(saved: Option<[f32; 2]>, size: [f32; 2], floating: bool, was: Option<bool>) -> Step {
    match saved {
        _ if !floating => Step::Keep,
        Some(saved) if was != Some(true) && differs(saved, size) => Step::Restore(saved),
        Some(saved) if !differs(saved, size) => Step::Keep,
        _ => Step::Save,
    }
}

/// Hyprland's socket, which the window asks whether it floats and tells to resize.
#[cfg(target_os = "linux")]
mod hyprland {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    fn ask(request: &str) -> Option<String> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
        let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
        let path = std::path::Path::new(&dir)
            .join("hypr")
            .join(signature)
            .join(".socket.sock");
        let mut socket = UnixStream::connect(path).ok()?;
        socket.write_all(request.as_bytes()).ok()?;
        let mut reply = String::new();
        socket.read_to_string(&mut reply).ok()?;
        Some(reply)
    }

    /// Whether this process's window floats; `None` before it is mapped, or when Hyprland doesn't answer.
    pub fn floating() -> Option<bool> {
        let clients: serde_json::Value = serde_json::from_str(&ask("j/clients")?).ok()?;
        let pid = std::process::id();
        clients
            .as_array()?
            .iter()
            .find(|c| c["pid"].as_u64() == Some(pid.into()))?["floating"]
            .as_bool()
    }

    /// Sizes this process's window. `hyprctl dispatch` took Lua from 0.56, so the older form is the fallback.
    pub fn resize([w, h]: [f32; 2]) {
        let (w, h, pid) = (w.round(), h.round(), std::process::id());
        let lua = format!(
            "dispatch hl.dsp.window.resize({{ x = {w}, y = {h}, window = \"pid:{pid}\" }})"
        );
        if ask(&lua).as_deref() != Some("ok") {
            ask(&format!(
                "dispatch resizewindowpixel exact {w} {h},pid:{pid}"
            ));
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod hyprland {
    pub fn floating() -> Option<bool> {
        None
    }
    pub fn resize(_: [f32; 2]) {}
}

impl Drop for Memory {
    fn drop(&mut self) {
        self.files.flush(fastframe_file::FLUSH_TIMEOUT);
    }
}

/// More than the half pixel a window manager settles by.
fn differs(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() > 0.5 || (a[1] - b[1]).abs() > 0.5
}

/// A missing or broken file opens the window at its default size.
fn read(path: &Path) -> Geometry {
    let saved: Saved = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    Geometry {
        size: saved.size,
        position: saved.position,
        maximized: saved.maximized,
    }
}

fn encode(g: &Geometry) -> Vec<u8> {
    let saved = Saved {
        size: g.size,
        position: g.position,
        maximized: g.maximized,
    };
    serde_json::to_vec_pretty(&saved).expect("plain numbers encode")
}

/// Set when another launch asked for the window; the context is there to wake it.
#[derive(Default)]
pub struct Summons {
    asked: AtomicBool,
    ctx: OnceLock<egui::Context>,
}

impl Summons {
    pub fn attach(&self, ctx: &egui::Context) {
        let _ = self.ctx.set(ctx.clone());
    }

    /// Call every frame: brings the window forward once for each time it was asked for.
    pub fn answer(&self, ctx: &egui::Context) {
        if !self.asked.swap(false, Ordering::Relaxed) {
            return;
        }
        // Wayland drops a window's request to focus itself; niri can be asked instead.
        if fastframe_shell::window::raise_on_compositor() {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        // Elsewhere on Wayland, asking for attention is an xdg-activation request, which Hyprland with
        // `misc:focus_on_activate` and others answer by focusing the window.
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
            egui::UserAttentionType::Informational,
        ));
    }
}

/// Becomes the one running copy, or hands the launch to it. `None` means another copy has it: exit.
pub fn claim(demo: bool) -> Option<(fastframe_instance::Guard, Arc<Summons>)> {
    let slot = Slot::new(APP_ID);
    // The demo runs beside the real player, for screenshots.
    let slot = if demo { slot.scoped("demo") } else { slot };
    let summons = Arc::new(Summons::default());
    let handler = summons.clone();
    match slot.claim("show", move |request| {
        (request == "show").then(|| {
            handler.asked.store(true, Ordering::Relaxed);
            if let Some(ctx) = handler.ctx.get() {
                ctx.request_repaint();
            }
            "ok".to_owned()
        })
    }) {
        Claim::First(guard) => Some((guard, summons)),
        Claim::Running(_) | Claim::Declined => None,
        Claim::Unanswered => {
            eprintln!("radio4000: already running, but not answering");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopens_where_it_was_left() {
        let dir = std::env::temp_dir().join(format!("radio4000-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("window.json");
        let left = Geometry {
            size: Some([320.0, 480.0]),
            position: Some([40.0, 60.0]),
            maximized: true,
        };
        std::fs::write(&path, encode(&left)).unwrap();
        assert_eq!(read(&path), left);

        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(read(&path), Geometry::default());
        assert_eq!(read(&dir.join("missing.json")), Geometry::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hyprland_keeps_the_floating_size() {
        let left = Some([333.0, 444.0]);
        // Reopened tiled: the tile's size is not saved.
        assert_eq!(step(left, [1276.0, 704.0], false, None), Step::Keep);
        // Floated again at Hyprland's size: back to the user's.
        assert_eq!(
            step(left, [1290.0, 718.0], true, Some(false)),
            Step::Restore([333.0, 444.0])
        );
        // Opened floating at the saved size, then resized by the user.
        assert_eq!(step(left, [333.2, 444.0], true, None), Step::Keep);
        assert_eq!(step(left, [300.0, 444.0], true, Some(true)), Step::Save);
        assert_eq!(step(None, [300.0, 444.0], true, None), Step::Save);
    }
}
