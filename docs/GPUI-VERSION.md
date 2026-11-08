# GPUI version and upgrade policy

**Status:** pinned
**Recorded:** 2026-11-07

## Pin

```toml
[workspace.dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "db43f05e398036f08e544c03cbc068216c7a0aaa" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "db43f05e398036f08e544c03cbc068216c7a0aaa" }

# App crate: enable only the features the target needs.
[target.'cfg(target_os = "macos")'.dependencies]
gpui_platform = { workspace = true, features = ["font-kit"] }

[target.'cfg(any(target_os = "linux", target_os = "freebsd"))'.dependencies]
gpui_platform = { workspace = true, features = ["wayland", "x11"] }

[target.'cfg(target_os = "windows")'.dependencies]
gpui_platform = { workspace = true }
```

`db43f05e398036f08e544c03cbc068216c7a0aaa` is Zed `main` as inspected on
2026-11-06. This is the same pin pilcrow uses, chosen because it is known to
compile on this machine and includes the APIs berth needs:

- `gpui_platform::application()` and the platform feature split;
- AccessKit roles/labels (`Role`, `A11ySubtreeBuilder`);
- `Window::render_to_image()` and `HeadlessAppContext`;
- `Window::observe_window_appearance`;
- `EntityInputHandler` and `ElementInputHandler`;
- `uniform_list`, `canvas`, native menus, clipboard, `open_url`.

Re-verify any API before relying on a snippet from a different GPUI version.

## Why not crates.io

- `gpui` on crates.io is 0.2.2 (2025-10-22). It is behind Zed `main`, uses the
  old `Application::new()` entry point, and does not include the
  `gpui_platform` split.
- `gpui_platform` is not published on crates.io.
- `gpui-pre` is a third-party snapshot and a different dependency family. Do
  not mix it with official GPUI crates.

## Platform features

- **macOS:** `font-kit` is required; without it glyphs lay out but do not draw.
  Metal is the renderer.
- **Linux:** at least one of `wayland` / `x11`; enable both for CI. wgpu renders
  through Vulkan.
- **Windows:** no extra features. Compile-only for now.
- `test-support` belongs in `[dev-dependencies]`, never in the app's normal
  features.

## Verified API notes for this pin

- Entry: `gpui_platform::application().with_quit_mode(..).run(|cx| { .. })`.
- Window: `cx.open_window(WindowOptions { window_bounds, titlebar,
  window_min_size, app_id, .. }, |window, cx| cx.new(..))`.
- View: `impl Render for BerthApp { fn render(&mut self, window, cx) -> impl
  IntoElement }`; call `cx.notify()` after state changes.
- Lists: `uniform_list(id, count, cx.processor(|this, range, window, cx| ..))`;
  `.track_scroll(&handle)` for a scroll handle.
- Canvas: `canvas(|bounds, window, cx| .., |bounds, state, window, cx| ..)`.
- Text input: implement `EntityInputHandler`; install with
  `window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity), cx)`.
- Menu: `cx.set_menus([Menu::new("File").items([..])])`.
- Appearance: `window.appearance()`, `window.observe_window_appearance(|window,
  cx| ..)`.
- Clipboard: `cx.write_to_clipboard(ClipboardItem::new_string(..))`.
- URL: `cx.open_url(..)`.
- Tests: `#[gpui::test]`, `TestAppContext`, `VisualTestContext`,
  `simulate_input`, `simulate_keystrokes`, `simulate_click`, `run_until_parked`.
- Headless render: `Window::render_to_image()` returns an
  `image::RgbaImage`; use `HeadlessAppContext` with the real text system.

## Known constraints

1. GPUI is pre-1.0. Pin, test, upgrade deliberately.
2. Text shaping is UI-thread work (`WindowTextSystem`/`TextSystem` use
   `Cell`/`RefCell`). Never share it across threads. Shape only visible rows.
3. `gpui_platform` is a git dependency. A clean build needs network; CI should
   cache the checkout and record the rev in artifacts.
4. Linux desktop menus are stored, not drawn. If the app needs a Linux menu,
   render it from the model.
5. AccessKit support exists but its coverage differs by platform; test the tree
   with xa11y on macOS and AT-SPI on Linux.
6. Headless rendering can be GPU-sensitive. Run screenshot jobs serially and
   skip with a visible reason if no renderer is available.

## Upgrade procedure

1. Pick a Zed commit; read the diff for `crates/gpui`, `crates/gpui_platform`
   and the platform crates.
2. Update **both** crates to the same rev in one change.
3. `cargo update -p gpui -p gpui_platform`; commit `Cargo.lock`.
4. Run `cargo test --workspace --locked` on macOS and Linux.
5. Run the headless screenshot matrix; read every changed image.
6. Run the live-engine smoke suite.
7. Update this file with the new rev, date, API notes and migrated code.
8. Prefer a local adapter over a fork for a cosmetic difference.

## Fallback

If the git dependency is operationally unacceptable, evaluate `gpui-pre` as a
single-family replacement. It is a crates.io snapshot, may lag Zed `main`, and
requires API adjustments. Do not mix families. Record the decision here and in
`docs/GPUI-REBUILD.md`.
