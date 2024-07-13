#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "java_manager")]
pub mod java_manager;
#[cfg(feature = "version_manager")]
pub mod version_manager;
#[cfg(feature = "bootstrap")]
pub mod bootstrap;

#[cfg(test)]
mod tests;
