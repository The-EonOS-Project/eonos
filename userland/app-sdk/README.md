# Era UI controls

`eonos_app::era::Window` provides safe drawing and interactive controls for
third-party Era applications. Controls are drawn each frame, so keep the same
control order from frame to frame to preserve keyboard focus.

Open a window with `Window::open(title, width, height)` to request a content
size, or `Window::open_auto(title)` to let Era fit it to the desktop with a
48-pixel margin around the window. `dimensions()` reports the accepted content
size. When a window is maximized or restored, `poll_event()` returns
`Event::Resized`; redraw using the updated dimensions for responsive layouts.
Keyboard activation of SDK buttons is opt-in through
`window.set_keyboard_widget_activation(true)`. It is off by default so an app's
own text editor or keyboard shortcuts do not also activate a focused button.

```rust
let mut enabled = true;
let mut choice_a = true;
let mut choice_b = false;
let mut volume = 50;
let mut name = String::new();

window.text(20, 20, 0x202020, "Settings")?;
window.checkbox(20, 52, "Enabled", &mut enabled)?;
if window.radio_button(20, 82, "Choice A", &mut choice_a)? {
    choice_b = false;
}
if window.radio_button(20, 108, "Choice B", &mut choice_b)? {
    choice_a = false;
}
window.slider(20, 142, 220, 30, 0, 100, &mut volume)?;
window.text_input(20, 184, 240, 30, &mut name, "Name")?;
window.progress_bar(20, 228, 220, 18, 3, 5)?;
window.present()?;
```

Buttons, dropdowns, checkboxes, text inputs, dialogs, toasts, and progress
dialogs are also available on `Window`. `radio_button` sets the selected value;
the caller clears other values in a group. `slider` changes its value on a
pointer click and clamps it to the supplied inclusive range. `progress_bar`
clamps values above its nonzero maximum.

`eonos_app::web::fetch` downloads a bounded HTTP(S) response through the
system's verified `curler` helper. TLS certificates are checked, HTTPS cannot
redirect down to HTTP, redirects are limited, and the response body is capped
at 256 KiB. Apps receive the body bytes, not a raw network socket. `fetch_html`
combines this with `parse_html`. The HTML parser provides a bounded, basic
HTML-to-text conversion. It removes markup and script/style bodies, decodes
common named and numeric entities, captures the document title, and records
link destinations with byte offsets into the normalized text. It is not a full
HTML or CSS browser engine. This is groundwork for a lightweight text browser,
not a full browser engine.

From the `eon-apps` repository root, build an external application with:

```sh
./app.sh build <app-directory>
```

Era applications can call `eonos_app::system::install_app_package(path)` to
install a validated `.eonapp` package from `/disk` or `/packages`, and
`eonos_app::system::uninstall_app(id)` to remove a user-installed app. Both
operations run through the system shell's package commands, require a writable
`/disk` mount, and return an error when the command reports a failure.
