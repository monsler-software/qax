//! Timer subscriptions driving an animation.
//!
//! A [`Component`] declares its timers from [`Component::subscriptions`], diffed
//! from state just like the view: a subscription runs exactly while it stays in
//! the returned list. Here a ~60 fps timer advances a spinning arc, but only
//! while playback is "running" — toggle it and the timer starts/stops with no
//! manual bookkeeping. This is the pattern a Winamp-style visualizer needs.
//!
//! Run with a display:  `cargo run -p qax --example timer`
use std::time::Duration;

use qax::ui::*;
use qax::Application;

/// A spinning arc whose angle is pure state — the timer just nudges it.
struct Spinner {
    angle: f32,
}
impl CustomWidget for Spinner {
    fn draw(&self, cx: &mut Canvas) {
        let (w, h) = cx.size();
        cx.clear(Color::rgb(24, 24, 28));
        let cx0 = w / 2;
        let cy0 = h / 2;
        let r = (w.min(h) / 2 - 8) as f32;
        // A dot orbiting the centre at the current angle.
        let x = cx0 as f32 + r * self.angle.cos();
        let y = cy0 as f32 + r * self.angle.sin();
        cx.fill_ellipse(x as i32 - 6, y as i32 - 6, 12, 12, Color::rgb(80, 200, 120));
        cx.stroke_rect(0, 0, w - 1, h - 1, 1, Color::rgb(90, 90, 96));
    }
    fn size(&self) -> Option<(i32, i32)> {
        Some((160, 160))
    }
}

#[derive(Clone)]
enum Msg {
    Tick,
    Toggle(bool),
}

#[derive(Default)]
struct State {
    angle: f32,
    running: bool,
    frames: u64,
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Tick => {
                self.angle += 0.12;
                self.frames += 1;
            }
            Msg::Toggle(on) => self.running = on,
        }
    }

    fn view(&self) -> Element<Msg> {
        column()
            .spacing(10)
            .padding(16)
            .child(custom::<Msg, _>(Spinner { angle: self.angle }))
            .child(label(format!("frames: {}", self.frames)))
            .child(
                checkbox("Run animation")
                    .checked(self.running)
                    .on_toggle(Msg::Toggle),
            )
            .child(button("Quit").on_click(Msg::Toggle(false)))
            .into_element()
    }

    fn subscriptions(&self) -> Vec<Subscription<Msg>> {
        // ~60 fps while running; nothing at all when paused.
        if self.running {
            vec![every(Duration::from_millis(16), Msg::Tick)]
        } else {
            vec![]
        }
    }
}

fn main() {
    let app = Application::new();
    let _ui = Ui::new(State {
        running: true,
        ..State::default()
    })
    .title("qax — timer")
    .size(200, 300)
    .mount();
    std::process::exit(app.exec());
}
