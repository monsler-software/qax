//! Background work feeding the UI through a thread-safe [`Emitter`].
//!
//! Clicking "Download" spawns a worker thread that simulates a slow, chunked
//! transfer and reports progress back with `emitter.emit(..)`. The reactive
//! runtime stays single-threaded — `update`/`view` only ever run on the GUI
//! thread — while the work happens off it. This is the shape of a real network
//! download, a file decode, or any async task.
//!
//! Run with a display:  `cargo run -p qax --example async`
use std::time::Duration;

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Start,
    Progress(i32),
    Done,
}

#[derive(Default)]
struct State {
    progress: i32,
    running: bool,
    done: bool,
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Start => {
                self.running = true;
                self.done = false;
                self.progress = 0;
            }
            Msg::Progress(p) => self.progress = p,
            Msg::Done => {
                self.running = false;
                self.done = true;
            }
        }
    }

    fn view(&self) -> Element<Msg> {
        let status = if self.done {
            "Done!".to_string()
        } else if self.running {
            format!("Downloading… {}%", self.progress)
        } else {
            "Idle".to_string()
        };
        column()
            .spacing(10)
            .padding(16)
            .child(label(status))
            .child(progress_bar(0, 100, self.progress))
            .child(
                button(if self.running { "Working…" } else { "Download" })
                    .on_click(Msg::Start),
            )
            .into_element()
    }
}

fn main() {
    let app = Application::new();
    let ui = Ui::new(State::default())
        .title("qax — async")
        .size(300, 160)
        .mount();

    // Hand a thread-safe emitter to a worker thread. It streams progress back
    // into the reactive runtime from off the GUI thread. To keep the example
    // self-driving it runs one pass shortly after startup; a real app would spawn
    // this from the `Msg::Start` click handler instead.
    let tx = ui.emitter();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        tx.emit(Msg::Start);
        for p in (0..=100).step_by(4) {
            std::thread::sleep(Duration::from_millis(40));
            tx.emit(Msg::Progress(p));
        }
        tx.emit(Msg::Done);
    });

    std::process::exit(app.exec());
}
