
use pelite::resources::version_info::{Language, VersionInfo};
use pelite::PeFile;
use std::path::Path;

const LANG_NEUTRAL_UNICODE: Language = Language {
    lang_id: 0x0000,
    charset_id: 0x04b0,
};

pub fn product_version(path: &Path) -> Option<String> {
    let map = pelite::FileMap::open(path).ok()?;
    let info: VersionInfo = PeFile::from_bytes(map.as_ref())
        .ok()?
        .resources()
        .ok()?
        .version_info()
        .ok()?;

    info.value(LANG_NEUTRAL_UNICODE, "ProductVersion")
        .map(|v| v.to_string())
}
