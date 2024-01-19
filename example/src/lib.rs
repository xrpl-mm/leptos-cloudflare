mod app;

#[cfg(feature = "hydrate")]
#[wasm_bindgen]
pub fn hydrate() {
    use app::App;
    use leptos::{mount_to_body, view};

    use wasm_bindgen::prelude::wasm_bindgen;
    use web_sys::console;

    console_error_panic_hook::set_once();
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    // Verify wasm hydrate binding was called on the client
    console::log_1(&"Preparing to mount client...".into());

    mount_to_body(|| {
        view! { <App /> }
    });
}
