pub mod html;
pub mod browser;

pub use html::html_to_plain_text;
pub use browser::build_preview_html_file;
pub use browser::open_in_browser;