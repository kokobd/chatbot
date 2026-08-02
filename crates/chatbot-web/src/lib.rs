#[cfg(any(feature = "ssr", feature = "hydrate"))]
mod frontend;

#[cfg(not(feature = "hydrate"))]
mod server;

#[cfg(not(feature = "hydrate"))]
pub use server::*;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    leptos::mount::mount_to_body(frontend::App);
}
