//! A mini Winamp-style player exercising the whole widget surface: a timer-driven
//! visualizer painted with gradients, transforms, paths and antialiasing; wheel
//! and mouse handlers on the canvas; a playlist ([`list`]); a menu bar whose
//! actions open modal [`dialog`]s; and a status bar. It plays nothing — it's a UI
//! showcase — but it's the shape a real port (e.g. Kalorite) would take.
//!
//! Run with a display:  `cargo run -p qax --example player`
use std::time::Duration;

use qax::dialog;
use qax::ui::*;
use qax::Application;

// ---- the animated visualizer (a custom-drawn widget) ----------------------

/// Draws a row of bars whose heights sweep with `phase`, under a spinning
/// "now playing" indicator. Pure function of its inputs; the timer just advances
/// `phase`, and the diff repaints.
struct Visualizer {
    phase: f32,
    bars: usize,
    volume: f32,
}
impl CustomWidget for Visualizer {
    fn draw(&self, cx: &mut Canvas) {
        let (w, h) = cx.size();
        cx.set_antialiasing(true);
        // Background gradient.
        cx.fill_rect_linear(
            0, 0, w, h,
            0.0, 0.0, Color::rgb(18, 18, 26),
            0.0, h as f64, Color::rgb(30, 30, 48),
        );

        // Spectrum bars with a vertical gradient each.
        let bw = (w as f32 / self.bars as f32).max(1.0);
        for i in 0..self.bars {
            let t = i as f32 / self.bars as f32;
            let amp = 0.5 + 0.5 * (self.phase + t * std::f32::consts::TAU).sin();
            let bh = (amp * self.volume * (h as f32 - 20.0)) as i32;
            let x = (i as f32 * bw) as i32;
            cx.fill_rect_linear(
                x, h - bh, bw as i32 - 2, bh,
                0.0, (h - bh) as f64, Color::rgb(80, 220, 140),
                0.0, h as f64, Color::rgb(40, 120, 200),
            );
        }

        // A spinning indicator in the top-right, built from a path + transforms.
        cx.save();
        cx.translate((w - 24) as f64, 24.0);
        cx.rotate((self.phase * 40.0) as f64);
        let mut p = Path::new();
        p.move_to(0.0, -12.0)
            .cubic_to(8.0, -8.0, 8.0, 8.0, 0.0, 12.0)
            .cubic_to(-8.0, 8.0, -8.0, -8.0, 0.0, -12.0)
            .close();
        cx.fill_path(&p, Color::rgba(240, 200, 90, 220));
        cx.restore();

        cx.stroke_rect(0, 0, w - 1, h - 1, 1, Color::rgb(70, 70, 90));
    }
    fn size(&self) -> Option<(i32, i32)> {
        Some((360, 140))
    }
}

// ---- app -------------------------------------------------------------------

#[derive(Clone)]
enum Msg {
    Tick,
    TogglePlay,
    Volume(i32), // wheel delta
    Seek(i32),   // click x on the visualizer
    Select(i32),
    AddTrack,
    OpenFiles,
    SavePlaylist,
    ClearPlaylist,
    Quit,
}

struct State {
    playing: bool,
    phase: f32,
    volume: f32,
    tracks: Vec<String>,
    current: i32,
    status: String,
}
impl Default for State {
    fn default() -> Self {
        State {
            playing: true,
            phase: 0.0,
            volume: 0.7,
            tracks: vec!["opening.mod".into(), "chiptune.xm".into()],
            current: 0,
            status: "ready".into(),
        }
    }
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Tick => self.phase += 0.15,
            Msg::TogglePlay => {
                self.playing = !self.playing;
                self.status = if self.playing { "playing" } else { "paused" }.into();
            }
            Msg::Volume(delta) => {
                self.volume = (self.volume + delta as f32 / 1200.0).clamp(0.0, 1.0);
                self.status = format!("volume {:.0}%", self.volume * 100.0);
            }
            Msg::Seek(x) => self.status = format!("seek to x={x}"),
            Msg::Select(i) => {
                self.current = i;
                if let Some(t) = self.tracks.get(i as usize) {
                    self.status = format!("selected {t}");
                }
            }
            Msg::AddTrack => {
                if let Some(name) = dialog::input("Add track", "File name:", "track.mod")
                    .filter(|n| !n.is_empty())
                {
                    self.tracks.push(name);
                }
            }
            Msg::OpenFiles => {
                if let Some(path) =
                    dialog::open_file("Open track", "", "Audio (*.mod *.wav *.mp3);;All files (*)")
                {
                    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                    self.status = format!("opened {name}");
                    self.tracks.push(name);
                }
            }
            Msg::SavePlaylist => {
                if let Some(path) = dialog::save_file("Save playlist", "", "Playlist (*.m3u)") {
                    self.status = format!("saved {} tracks to {path}", self.tracks.len());
                }
            }
            Msg::ClearPlaylist => {
                if dialog::confirm("Clear playlist", "Remove all tracks?") {
                    self.tracks.clear();
                    self.current = -1;
                }
            }
            Msg::Quit => std::process::exit(0),
        }
    }

    fn view(&self) -> Element<Msg> {
        column()
            .spacing(8)
            .padding(10)
            .child(
                custom::<Msg, _>(Visualizer {
                    phase: self.phase,
                    bars: 48,
                    volume: self.volume,
                })
                .on_mouse_down(|e: MouseEvent| Msg::Seek(e.x))
                .on_wheel(|e: WheelEvent| Msg::Volume(e.delta)),
            )
            .child(
                row()
                    .spacing(8)
                    .child(
                        button(if self.playing { "Pause" } else { "Play" })
                            .on_click(Msg::TogglePlay)
                            .default(true)
                            .tooltip("Play / pause the track")
                            .stylesheet(
                                "QPushButton { background: #2d7d46; color: white; \
                                 border: none; border-radius: 6px; padding: 6px 18px; } \
                                 QPushButton:hover { background: #35924f; } \
                                 QPushButton:pressed { background: #256b3b; }",
                            ),
                    )
                    .child(label(format!("Vol {:.0}%", self.volume * 100.0)))
                    .child(progress_bar(0, 100, (self.volume * 100.0) as i32)),
            )
            .child(
                list()
                    .items(self.tracks.clone())
                    .selected(self.current)
                    .on_select(Msg::Select)
                    .on_activate(Msg::Select),
            )
            .into_element()
    }

    fn subscriptions(&self) -> Vec<Subscription<Msg>> {
        if self.playing {
            vec![every(Duration::from_millis(33), Msg::Tick)]
        } else {
            vec![]
        }
    }
}

fn main() {
    let app = Application::new();
    let _ui = Ui::new(State::default())
        .title("qax — player")
        .size(400, 400)
        .status("ready")
        .menu(
            menu("Playlist")
                .action("Add track…", Msg::AddTrack)
                .action("Open file…", Msg::OpenFiles)
                .action("Save playlist…", Msg::SavePlaylist)
                .action("Clear", Msg::ClearPlaylist)
                .separator()
                
                .action("Quit", Msg::Quit),
        )
        .menu(menu("Playback").action("Play / Pause", Msg::TogglePlay))
        .mount();
    std::process::exit(app.exec());
}
