//! Verifies timer subscriptions: a timer runs exactly while it stays in the list
//! returned by `subscriptions`, and stops once the state drops it. Runs headless
//! (offscreen) via `Application::run_for`, which spins the real event loop for a
//! bounded time so QTimers actually fire.

use std::time::Duration;

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Tick,
    SetRunning(bool),
}

#[derive(Default)]
struct State {
    ticks: u32,
    running: bool,
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Tick => self.ticks += 1,
            Msg::SetRunning(on) => self.running = on,
        }
    }

    fn view(&self) -> Element<Msg> {
        column()
            .child(label(format!("ticks {}", self.ticks)))
            .into_element()
    }

    fn subscriptions(&self) -> Vec<Subscription<Msg>> {
        if self.running {
            vec![every(Duration::from_millis(5), Msg::Tick)]
        } else {
            vec![]
        }
    }
}

#[test]
fn timer_runs_only_while_subscribed() {
    unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    let app = Application::new();

    let ui = Ui::new(State::default()).title("t").size(80, 40).mount();

    // Not subscribed yet: the loop turning should produce no ticks.
    app.run_for(40);
    assert_eq!(count(&ui), 0, "no timer before subscribing");

    // Subscribe (state change re-runs subscriptions -> starts the 5ms timer).
    ui.dispatch(Msg::SetRunning(true));
    app.run_for(80);
    let while_running = count(&ui);
    assert!(
        while_running >= 3,
        "timer should tick several times while subscribed, got {while_running}"
    );

    // Unsubscribe: the timer must stop. Drain any tick already queued before the
    // stop (still delivered — the message was emitted), then confirm no more.
    ui.dispatch(Msg::SetRunning(false));
    app.run_for(20);
    let at_stop = count(&ui);
    app.run_for(60);
    assert_eq!(
        count(&ui),
        at_stop,
        "timer must stop once dropped from subscriptions"
    );
}

fn count(ui: &Ui<State>) -> u32 {
    ui.state(|s| s.ticks)
}
