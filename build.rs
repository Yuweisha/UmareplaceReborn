
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);

    let rc = format!(
        r#"#include <windows.h>

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x2
{{
    BLOCK "StringFileInfo"
    {{
        BLOCK "000004b0"
        {{
            VALUE "ProductName", "UmareplaceReborn"
            VALUE "FileVersion", "{version}"
            VALUE "ProductVersion", "{version}"
            VALUE "FileDescription", "UmareplaceReborn - Hachimi character replacement plugin"
        }}
    }}
    BLOCK "VarFileInfo"
    {{
        VALUE "Translation", 0x0000 0x04B0
    }}
}}
"#
    );

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("plugin_version.rc");
    std::fs::write(&out, rc).expect("写入版本资源失败");

    #[cfg(windows)]
    embed_resource::compile(&out, embed_resource::NONE);
}
