use std::path::Path;
pub fn verify(path: &Path) -> anyhow::Result<()> {
    crate::macho::verify(path)
}
