//! gallery-dl backend for image gallery and booru site downloads.
//!
//! gallery-dl is a command-line program to download image galleries and
//! collections from several image hosting sites. This backend wraps it
//! following the same subprocess pattern as the yt-dlp backend.

#[cfg(not(target_os = "android"))]
mod process;

#[cfg(not(target_os = "android"))]
pub use process::GalleryDlBackend;
