//! Custom-drawn widgets + translations.
//!
//! `Meter` is a user-defined [`CustomWidget`]: a bespoke, painted widget the
//! built-in set doesn't provide. It draws itself into a safe [`Canvas`] — no raw
//! pointers, no `unsafe` — and composes into the reactive tree exactly like
//! `label`/`button`. The diff keeps its widget alive across renders and just
//! repaints it with new data. User-facing strings are wrapped in [`tr!`] so
//! `cargo qax i18n` can extract them.
//!
//! Run with a display:  `cargo run -p qax --example custom`
use qax::tr;
use qax::ui::*;
use qax::Application;

/// A horizontal level meter: a filled bar proportional to `value` (0.0–1.0).
struct Meter {
    value: f32,
}
impl CustomWidget for Meter {
    fn draw(&self, cx: &mut Canvas) {
        let (w, h) = cx.size();
        cx.clear(Color::rgb(30, 30, 34));
        let filled = (self.value.clamp(0.0, 1.0) * w as f32) as i32;
        cx.fill_rect(0, 0, filled, h, Color::rgb(80, 200, 120));
        cx.stroke_rect(0, 0, w - 1, h - 1, 1, Color::rgb(90, 90, 96));
    }
    fn size(&self) -> Option<(i32, i32)> {
        Some((260, 24))
    }
}

/// The meter's fixed width (see [`Meter::size`]); handlers use it to turn a click
/// x-coordinate into a 0.0–1.0 level.
const METER_W: i32 = 260;

/// Custom widgets stay ergonomic: expose them as a plain function returning an
/// element, and they drop into `view` like any built-in. Here the meter is also
/// interactive — click or drag anywhere on it to set the level, mapping the
/// mouse x-coordinate to a fraction of its width.
fn meter(value: f32) -> impl IntoElement<Msg> {
    let to_level = |e: MouseEvent| Msg::SetLevel((e.x as f32 / METER_W as f32).clamp(0.0, 1.0));
    custom(Meter { value })
        .on_mouse_down(to_level)
        .on_mouse_move(to_level)
}

#[derive(Clone)]
enum Msg {
    Louder,
    Quieter,
    SetLevel(f32),
}

#[derive(Default)]
struct State {
    level: f32,
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Louder => self.level = (self.level + 0.1).min(1.0),
            Msg::Quieter => self.level = (self.level - 0.1).max(0.0),
            Msg::SetLevel(v) => self.level = v,
        }
    }

    fn view(&self) -> Element<Msg> {
        column()
            .spacing(10)
            .padding(16)
            .child(label(tr!("Now playing")))
            .child(meter(self.level))
            .child(label(format!("{}: {:.0}%", tr!("Level"), self.level * 100.0)))
            .child(label(tr!("Click or drag the bar to set the level")))
            .child(
                row()
                    .spacing(8)
                    .child(button(tr!("−")).on_click(Msg::Quieter))
                    .child(button(tr!("+")).on_click(Msg::Louder)),
            )
            .into_element()
    }
}

fn main() {
    let app = Application::new();
    // If a catalogue exists it applies automatically; otherwise the original
    // strings are shown. The `.qm` is compiled from `translations/qax_ru.ts`
    // into OUT_DIR by build.rs (via qax-build) during `cargo build`.
    let _ru = qax::i18n::load_translation(concat!(env!("OUT_DIR"), "/qax_ru.qm"));
    let _ui = Ui::new(State::default())
        .title(tr!("Player"))
        .size(300, 200)
        .mount();
    std::process::exit(app.exec());
}
