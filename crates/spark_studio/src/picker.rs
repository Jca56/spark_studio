//! Native open/save dialogs: spawn lntrn-file-manager in pick mode on a
//! worker thread. Its contract: chosen path(s) print to stdout one per
//! line; cancelling exits with code 1. The result comes back to the event
//! loop as an [`AppEvent::Picked`].

use std::path::{Path, PathBuf};
use std::process::Command;

use winit::event_loop::EventLoopProxy;

use crate::AppEvent;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Purpose {
    OpenComp,
    SaveComp,
    ImportAudio,
    /// Write the selection to a reusable .sparkshape file.
    SaveShape,
    /// Append a saved .sparkshape (or another comp's shapes) to this comp.
    ImportShape,
    /// Bring a glTF model in as a mesh object.
    ImportMesh,
}

pub fn spawn(proxy: EventLoopProxy<AppEvent>, purpose: Purpose, current_file: &str) {
    let mut cmd = Command::new("lntrn-file-manager");
    match purpose {
        Purpose::OpenComp => {
            cmd.args(["--pick", "--title", "Open comp"])
                .args(["--filters", "Spark comps:*.spark"]);
        }
        Purpose::SaveComp => {
            let name = Path::new(current_file)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "comp.spark".into());
            cmd.args(["--pick-save", "--title", "Save comp as"])
                .args(["--filters", "Spark comps:*.spark"])
                .args(["--save-name", &name]);
        }
        Purpose::ImportAudio => {
            cmd.args(["--pick", "--title", "Import audio"]).args([
                "--filters",
                "Audio:*.mp3,*.wav,*.flac,*.ogg,*.m4a,*.opus|All files:*",
            ]);
        }
        Purpose::SaveShape => {
            cmd.args(["--pick-save", "--title", "Save shape as"])
                .args(["--filters", "Spark shapes:*.sparkshape"])
                .args(["--save-name", "shape.sparkshape"]);
        }
        Purpose::ImportShape => {
            cmd.args(["--pick", "--title", "Import shape"]).args([
                "--filters",
                "Spark shapes:*.sparkshape|Spark comps:*.spark|All files:*",
            ]);
        }
        Purpose::ImportMesh => {
            cmd.args(["--pick", "--title", "Import mesh"])
                .args(["--filters", "Meshes:*.glb,*.gltf|All files:*"]);
        }
    }
    if let Ok(dir) = std::env::current_dir() {
        cmd.arg("--start-dir").arg(dir);
    }
    std::thread::spawn(move || {
        let path = match cmd.output() {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(PathBuf::from),
            Ok(_) => None, // cancelled
            Err(e) => {
                println!("file picker failed to launch: {e}");
                None
            }
        };
        let _ = proxy.send_event(AppEvent::Picked(purpose, path));
    });
}
