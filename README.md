# qax

High-level, maintainable Qt 6 bindings for Rust. Supports **loading QML** and
**composing components from Rust code**, wiring both together with a reactive,
idiomatic Rust API.

## Workspace layout

```
qax/
├── crates/
│   ├── qax-sys/          # low-level FFI: a narrow C ABI over Qt6
│   │   ├── cpp/shim.{h,cpp}   # hand-written C++ glue (one flat fn per op)
│   │   ├── build.rs           # pkg-config + cc; no moc codegen needed
│   │   └── src/lib.rs         # extern "C" declarations
│   ├── qax/              # safe, ergonomic API
│   │   ├── src/app.rs         # Application  (QGuiApplication + event loop)
│   │   ├── src/engine.rs      # QmlEngine    (QQmlApplicationEngine)
│   │   ├── src/model.rs       # Model        (QQmlPropertyMap bridge)
│   │   ├── src/ui.rs          # ui::*        (state-driven, diffing widget tree)
│   │   ├── src/i18n.rs        # tr! / translations / embedded resources
│   │   ├── src/value.rs       # Value / IntoValue
│   │   ├── src/reactive.rs    # Property<T>  (Qt-free observable state)
│   │   ├── examples/counter.rs   # QML path
│   │   ├── examples/widgets.rs   # code-driven, state-driven path
│   │   ├── examples/dynamic.rs   # a list derived from data
│   │   └── examples/custom.rs    # custom widgets + tr! strings
│   ├── qax-build/       # build-script helper: .ts → .qm and .qrc → .rcc at build time
│   └── cargo-qax/        # `cargo qax` subcommand: tr! extraction + one-off .qrc → .rcc
```

## Architecture

```
your app ──► qax (safe) ──► qax-sys (raw FFI) ──► cpp/shim ──► Qt6
```

### Smart components: pay only for what you use

There are **no cargo features to toggle** — every widget is always available in
the API. Yet a widget you never call contributes nothing to your binary. The
shim is compiled with `-ffunction-sections -fdata-sections`, and the final
binary is linked with `--gc-sections` + `--as-needed` (see `.cargo/config.toml`).
So Rust's own dead-code elimination plus the linker drop:

- every wrapper function nothing references, and
- transitively, every Qt shared library that only that dead glue needed.

Measured on the two examples in this repo:

| binary            | Qt libs linked (`ldd`)                        |
|-------------------|-----------------------------------------------|
| `widgets` (no QML)| Core, Gui, Widgets — **no Qml/Quick**         |
| `counter` (QML)   | Core, Gui, Qml, Widgets                        |

`nm` confirms it at function granularity: `counter` contains no
`qt_slider_new`/`qt_checkbox_new`; `widgets` contains no `qt_qml_engine_new`.
Use a widget → it's linked; ignore it → it's gone, automatically.

Design principles that keep the binding maintainable:

- **One narrow C ABI.** Every Qt type crosses the boundary as an opaque pointer.
  Adding a class = adding a few flat `extern "C"` functions to the shim; there is
  no generated glue to keep in sync.
- **No per-type moc.** User types are exposed to QML through Qt's own
  `QQmlPropertyMap`, and Rust callbacks connect to Qt signals via Qt 6's
  functor-`connect` (no moc on the receiver). So the C++ shim builds with a plain
  `cc` invocation — no `moc` step in `build.rs`.
- **Reactive both ways.** A `Model` field written from QML *or* Rust flows
  through a single `on_change` channel of ordinary Rust closures.
- **Testable logic layer.** `reactive::Property<T>` carries no Qt dependency, so
  application state can be composed and unit-tested without an event loop.

## Two ways to build a UI

**Load QML, bind a Rust model** (see `examples/counter.rs`):

```rust
let app = Application::new();
let mut engine = QmlEngine::new();

let mut backend = Model::new();
backend.set("clicks", 0i64);
backend.on_change(|key, fields| println!("{key} = {:?}", fields.get(key)));

engine.set_context("backend", &backend);
engine.load_file("ui/main.qml");   // or load_data(SRC, "main.qml")
app.exec();
```

**Build the UI from code, state-driven** — no QML, no handles. You describe the
UI as a pure function of your state; the library diffs successive trees and
mutates only what changed (`qax::ui`, see `examples/widgets.rs`):

```rust
use qax::{Application, ui::*};

#[derive(Clone)]
enum Msg { Inc, Dec }

#[derive(Default)]
struct Counter { n: i64 }

impl Component for Counter {
    type Message = Msg;
    fn update(&mut self, msg: Msg) {
        match msg { Msg::Inc => self.n += 1, Msg::Dec => self.n -= 1 }
    }
    fn view(&self) -> Element<Msg> {
        column().spacing(12).padding(16)
            .child(label(format!("Count: {}", self.n)))
            .child(row().spacing(8)
                .child(button("−").on_click(Msg::Dec))
                .child(button("+").on_click(Msg::Inc)))
            .into_element()
    }
}

let app = Application::new();
let _ui = Ui::new(Counter::default()).title("counter").size(320, 200).mount();
app.exec();
```

This is the Elm architecture. Events become `Message` values; `update` applies a
message to your data; then `view` runs again and the runtime **reconciles** the
new `Element` tree against the widgets on screen — patching text, values and
handlers in place, inserting/removing only the children that actually differ.
Widgets are never rebuilt when a prop merely changes (verified in
`tests/diff.rs`). Re-renders are deferred to the next event-loop turn, so a diff
never runs on the stack of the signal handler that triggered it.

Available builders: `label`, `button`, `checkbox`, `line_edit`, `slider`,
`spinbox`, `progress_bar`, `combo_box`, plus `column`/`row` containers. New
widgets are a self-contained addition: a few flat functions in `shim.cpp`, their
FFI decls, and one builder + diff arm in `ui.rs`.

**Dynamic trees fall out for free.** There is no imperative `add_child`/`clear`:
a list is just data. Push onto a `Vec` in `update` and render it in `view`; the
diff inserts the one new child (see `examples/dynamic.rs`):

```rust
let list = column().spacing(6)
    .children(self.items.iter().map(|&n| button(format!("Item #{n}")).on_click(Msg::Clicked(n))));
```

**Custom widgets** compose just as elegantly. A reusable piece built from
existing widgets is simply a function returning `impl IntoElement<Msg>` — no
trait needed. For bespoke rendering (a spectrum, a level meter, a `visualizer`),
implement `CustomWidget` and paint into a **safe `Canvas`** — no raw pointers, no
`unsafe`. The diff keeps the widget alive across renders and just repaints it
with the new data (see `examples/custom.rs`):

```rust
struct Visualizer { bars: Vec<f32> }
impl CustomWidget for Visualizer {
    fn draw(&self, cx: &mut Canvas) {
        cx.clear(Color::BLACK);
        let (w, h) = cx.size();
        let bw = w / self.bars.len().max(1) as i32;
        for (i, &v) in self.bars.iter().enumerate() {
            let bh = (v * h as f32) as i32;
            cx.fill_rect(i as i32 * bw, h - bh, bw - 2, bh, Color::rgb(80, 200, 120));
        }
    }
}
fn visualizer<Msg>(bars: &[f32]) -> impl IntoElement<Msg> { custom(Visualizer { bars: bars.to_vec() }) }

// ...in view:
row().child(audio_display(&self.meta, &self.spectrum)).child(visualizer(&self.spectrum))
```

`Canvas` offers `fill_rect`/`stroke_rect`/`fill_ellipse`/`line`/`text`/`clear`
over `Color::rgb(..)`/`rgba(..)`. The raw `QPainter` never leaves the shim.

**Compose state from code** with `Property<T>` for the Qt-free logic layer, or
just keep it in your `Component`.

## Translations

Wrap user-facing strings in `tr!` — `tr!("text")` or `tr!("Context", "text")`.
At runtime each call resolves through the installed Qt catalogues (falling back
to the original text); at build time `cargo qax i18n` scans your sources for
`tr!` calls and generates/merges Qt Linguist `.ts` files, keeping translations
already filled in:

```sh
cargo install --path crates/cargo-qax      # provides the `cargo qax` subcommand
cargo qax i18n --lang ru,en                # writes translations/<crate>_<lang>.ts
# translators fill in the .ts files, then `cargo build` compiles them to .qm
```

The `.ts → .qm` compilation (and `.qrc → .rcc`, below) is wired into the build by
the `qax-build` helper — no copy-pasted build script. Add it as a build
dependency and drive it from your `build.rs`:

```toml
# Cargo.toml
[build-dependencies]
qax-build = "0.1"
```

```rust
// build.rs
fn main() {
    qax_build::Build::new()
        .translations("translations", ["ru", "en"]) // *.ts -> OUT_DIR/*.qm
        .resource("assets/resources.qrc")            // *.qrc -> OUT_DIR/resources.rcc
        .run();
}
```

`run()` compiles whatever `.ts`/`.qrc` files exist, writes the artifacts to
`OUT_DIR`, and emits `cargo:rerun-if-changed`. It's best-effort: a checkout
without `lrelease`/`rcc` still builds (emitting a `cargo:warning`), it just lacks
translations/resources. `qax-build` finds Qt's tools even when they aren't on
`PATH` (e.g. `/usr/lib/qt6/rcc` on many Linux distros). At runtime, load from
`OUT_DIR`:

```rust
let _ru = qax::i18n::load_translation(concat!(env!("OUT_DIR"), "/qax_ru.qm"));
let title = tr!("Now playing");
```

## Embedded resources

`Build::resource()` (above) compiles a `.qrc` into a binary bundle at build time;
register it from memory so images, fonts and QML ship inside the executable:

```rust
static RES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/resources.rcc"));
qax::i18n::register_resource(RES);   // files now reachable under :/
```

For a one-off compile outside the build, the CLI still works:
`cargo qax qrc assets/resources.qrc -o resources.rcc`.

## Requirements

- Qt 6 with `pkg-config` files for `Qt6Core`, `Qt6Gui`, `Qt6Qml`, `Qt6Quick`.
- A C++17 compiler.

## Run the example

```sh
cargo run -p qax --example counter   # QML window bound to a Rust Model
cargo run -p qax --example widgets   # state-driven tree built in Rust code
cargo run -p qax --example dynamic   # a list derived from data
cargo run -p qax --example custom    # custom widgets + tr! translations
# headless smoke test:
QT_QPA_PLATFORM=offscreen cargo run -p qax --example counter
```

## Roadmap / extension points

- Richer `Value` variants (lists, nested models) → `QVariantList` / list models.
- `QAbstractListModel` wrapper for efficient collection views.
- Invokable Rust methods callable from QML (beyond property write-back).
- Registering Rust-defined QML types (`qmlRegisterType`).

Each lands as a self-contained addition to `shim.{h,cpp}` plus a safe wrapper.

## License

MIT OR Apache-2.0
