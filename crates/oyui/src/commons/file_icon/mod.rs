pub mod devicon;

pub trait FileIconProvider: std::fmt::Debug  {
    /// Returns the icon character associated with the file name.
    fn get_file_icon(&self, name: &str) -> char;
}

pub use devicon::DevIconProvider;

