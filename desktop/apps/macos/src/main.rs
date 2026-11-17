//! The island as a native macOS app (AppKit through objc2). For Macs
//! without a notch (#18): on a screen with one, the system already has its
//! own, so the island stays away from it.

#[cfg(target_os = "macos")]
mod app;
mod geometry;

fn main() {
    #[cfg(target_os = "macos")]
    app::run();
}
