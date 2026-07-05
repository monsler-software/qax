//! Multi-window app: a second window is opened *by an event* — and cleaned up
//! when it is closed, so windows don't pile up.
//!
//! Each [`Ui`] is its own top-level window (a `QMainWindow`); several can live at
//! once and share the one [`Application`] event loop. A window stays open only as
//! long as its `Ui` handle is kept alive, so we store the spawned handles in the
//! main window's state — and drop a handle to close its window and reclaim it.
//!
//! Because `Component::update` runs on the GUI thread, it can build and `mount()`
//! a fresh window right there in response to a message. Each new window gets its
//! own independent state and reactive loop (note the per-window counter).
//!
//! Reclaiming closed windows is the interesting part. We can't drop a window's
//! handle from inside its own close event (that would delete it mid-dispatch), so
//! each child's [`Ui::on_close`] pokes the parent through an [`Emitter`]; the
//! parent then drops the handle on the next event-loop turn — safely deferred.
//!
//! Run with a display:  `cargo run -p qax --example multiwindow`

use qax::ui::*;
use qax::Application;

// ---- the pop-up window: its own component, with its own state ---------------

#[derive(Clone)]
enum ChildMsg {
    Bump,
}

struct Child {
    id: usize,
    clicks: u32,
}

impl Component for Child {
    type Message = ChildMsg;

    fn update(&mut self, msg: ChildMsg) {
        match msg {
            ChildMsg::Bump => self.clicks += 1,
        }
    }

    fn view(&self) -> Element<ChildMsg> {
        column()
            .padding(16)
            .spacing(12)
            .child(label(format!("I am window #{}", self.id)))
            .child(label(format!("Clicked {} time(s)", self.clicks)))
            .child(button("Click me").on_click(ChildMsg::Bump))
            .stretch()
            .into_element()
    }
}

// ---- the main window: opens child windows and reclaims closed ones ----------

#[derive(Clone)]
enum Msg {
    /// Delivered once at startup with the main window's own emitter, so child
    /// windows can call back into it.
    Ready(Emitter<Msg>),
    OpenWindow,
    /// A child window was closed; drop its handle to reclaim it.
    WindowClosed(usize),
}

#[derive(Default)]
struct Main {
    opened: usize,
    emitter: Option<Emitter<Msg>>,
    // Holding the handles keeps the windows alive; removing one closes its window
    // and frees all its state.
    windows: Vec<(usize, Ui<Child>)>,
}

impl Component for Main {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Ready(em) => self.emitter = Some(em),
            Msg::OpenWindow => {
                self.opened += 1;
                let id = self.opened;
                // The child reports its own closing back to us via the emitter.
                let em = self.emitter.clone().expect("emitter set at startup");
                let child = Ui::new(Child { id, clicks: 0 })
                    .title(format!("Window #{id}"))
                    .size(260, 160)
                    .position(140 + id as i32 * 30, 140 + id as i32 * 30)
                    .on_close(move || em.emit(Msg::WindowClosed(id)))
                    .mount();
                self.windows.push((id, child));
            }
            Msg::WindowClosed(id) => {
                // Dropping the handle deletes the window and frees its state. This
                // runs a turn after the close event, so tearing it down is safe.
                self.windows.retain(|(wid, _)| *wid != id);
            }
        }
    }

    fn view(&self) -> Element<Msg> {
        let open: Vec<usize> = self.windows.iter().map(|(id, _)| *id).collect();
        let list = format!(
            "Open windows: {}",
            if open.is_empty() {
                "none".to_string()
            } else {
                open.iter().map(|id| format!("#{id}")).collect::<Vec<_>>().join(", ")
            }
        );

        column()
            .padding(16)
            .spacing(12)
            .child(label(list))
            .child(button("Open a new window").on_click(Msg::OpenWindow))
            .child(label("(close a pop-up window to reclaim it)"))
            .stretch()
            .into_element()
    }
}

fn main() {
    let app = Application::new();

    let main = Ui::new(Main::default())
        .title("Multi-window — qax")
        .size(340, 200)
        .centered()
        .mount();

    // Hand the window its own emitter so children can call back into it.
    main.dispatch(Msg::Ready(main.emitter()));

    std::process::exit(app.exec());
}
