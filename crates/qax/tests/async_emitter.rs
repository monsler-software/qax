//! Verifies the cross-thread [`Emitter`]: messages sent from a background thread
//! are applied on the GUI thread and drive `update`/`view`, just like a widget
//! event. Runs headless (offscreen) via `Application::run_for`, which spins the
//! real event loop so the queued cross-thread callbacks fire.

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Add(i64),
}

#[derive(Default)]
struct State {
    sum: i64,
    count: u32,
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Add(n) => {
                self.sum += n;
                self.count += 1;
            }
        }
    }

    fn view(&self) -> Element<Msg> {
        column()
            .child(label(format!("sum {}", self.sum)))
            .into_element()
    }
}

#[test]
fn background_thread_feeds_the_ui() {
    unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    let app = Application::new();

    let ui = Ui::new(State::default()).title("t").size(80, 40).mount();
    let tx = ui.emitter();

    // Emit from several worker threads at once.
    let mut handles = Vec::new();
    for _ in 0..4 {
        let tx = tx.clone();
        handles.push(std::thread::spawn(move || {
            for i in 1..=25 {
                tx.emit(Msg::Add(i));
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // Let the GUI thread drain the queued messages.
    app.run_for(60);

    // 4 threads × sum(1..=25) = 4 × 325 = 1300, across 100 messages.
    assert_eq!(ui.state(|s| s.count), 100, "every emitted message applied");
    assert_eq!(ui.state(|s| s.sum), 1300, "values summed correctly");
}
