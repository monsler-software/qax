//! State-driven, GPUI/Elm-style UI — no QML, no handles.
//!
//! The whole screen is a pure function of `State`. Events become `Msg` values,
//! `update` mutates the state, and the library diffs the new tree against the
//! old one and touches only the widgets that changed.
//!
//! Run with a display:  `cargo run -p qax --example widgets`
use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Amount(i32),
    Repeat(i32),
    Color(i32),
    Echo(String),
    ToggleEcho(bool),
    Gain(f64),
    Angle(i32),
    Shape(usize, bool),
    Notes(String),
    Quit,
}

#[derive(Default)]
struct State {
    amount: i32,
    repeat: i32,
    color: i32,
    echo_on: bool,
    text: String,
    status: String,
    gain: f64,
    angle: i32,
    shape: usize,
    notes: String,
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Amount(v) => {
                self.amount = v;
                self.status = format!("Slider = {v}");
            }
            Msg::Repeat(v) => {
                self.repeat = v;
                self.status = format!("Repeat = {v}");
            }
            Msg::Color(i) => {
                self.color = i;
                self.status = format!("Color index = {i}");
            }
            Msg::ToggleEcho(on) => self.echo_on = on,
            Msg::Echo(t) => self.text = t,
            Msg::Gain(v) => {
                self.gain = v;
                self.status = format!("Gain = {v:.2}");
            }
            Msg::Angle(v) => {
                self.angle = v;
                self.status = format!("Angle = {v}°");
            }
            // A radio group fires toggled(false) for the one being deselected and
            // toggled(true) for the new pick; only react to the selection.
            Msg::Shape(i, true) => {
                self.shape = i;
                self.status = format!("Shape = {}", ["Circle", "Square", "Triangle"][i]);
            }
            Msg::Shape(_, false) => {}
            Msg::Notes(t) => self.notes = t,
            Msg::Quit => std::process::exit(0),
        }
    }

    fn view(&self) -> Element<Msg> {
        let status = if self.status.is_empty() {
            "Move the slider or type below".to_string()
        } else {
            self.status.clone()
        };
        let echo = if self.echo_on { self.text.as_str() } else { "" };

        column()
            .spacing(10)
            .padding(16)
            .child(label(status))
            .child(
                row()
                    .spacing(8)
                    .child(label("Amount:"))
                    .child(slider(0, 100, self.amount).on_change(Msg::Amount)),
            )
            .child(progress_bar(0, 100, self.amount))
            .child(
                row()
                    .spacing(8)
                    .child(label("Repeat:"))
                    .child(spinbox(1, 10, self.repeat.max(1)).on_change(Msg::Repeat))
                    .child(
                        combo_box()
                            .items(["Red", "Green", "Blue"])
                            .selected(self.color)
                            .on_change(Msg::Color),
                    ),
            )
            .child(checkbox("Enable echo").checked(self.echo_on).on_toggle(Msg::ToggleEcho))
            .child(
                line_edit()
                    .placeholder("Type here…")
                    .text(self.text.clone())
                    .on_change(|t: &str| Msg::Echo(t.to_string())),
            )
            .child(label(format!("echo: {echo}")))
            .child(separator())
            .child(
                row()
                    .spacing(12)
                    .child(
                        group_box("Signal")
                            .spacing(6)
                            .child(
                                row()
                                    .spacing(8)
                                    .child(label("Gain:"))
                                    .child(
                                        double_spinbox(0.0, 10.0, self.gain)
                                            .decimals(2)
                                            .step(0.25)
                                            .on_change(Msg::Gain),
                                    ),
                            )
                            .child(
                                row()
                                    .spacing(8)
                                    .child(label("Angle:"))
                                    .child(dial(0, 360, self.angle).on_change(Msg::Angle)),
                            ),
                    )
                    .child(
                        group_box("Shape")
                            .spacing(4)
                            .child(
                                radio_button("Circle")
                                    .checked(self.shape == 0)
                                    .on_toggle(|on| Msg::Shape(0, on)),
                            )
                            .child(
                                radio_button("Square")
                                    .checked(self.shape == 1)
                                    .on_toggle(|on| Msg::Shape(1, on)),
                            )
                            .child(
                                radio_button("Triangle")
                                    .checked(self.shape == 2)
                                    .on_toggle(|on| Msg::Shape(2, on)),
                            ),
                    ),
            )
            .child(label("Notes:"))
            .child(
                text_edit()
                    .placeholder("Multi-line notes…")
                    .text(self.notes.clone())
                    .on_change(|t: &str| Msg::Notes(t.to_string())),
            )
            .stretch()
            .child(button("Quit").on_click(Msg::Quit))
            .into_element()
    }
}

fn main() {
    let app = Application::new();
    let _ui = Ui::new(State {
        repeat: 1,
        echo_on: true,
        ..State::default()
    })
    .title("qax — widgets")
    .size(440, 620)
    .mount();
    std::process::exit(app.exec());
}
